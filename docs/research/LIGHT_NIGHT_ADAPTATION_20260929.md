# Light, night and eye adaptation for Panzerrechnersystem

Research report, 29 Sep 2026, written by the light researcher. The scope is Stalingrad, winter 1942–43,
at 48.708 N, 44.513 E (the DEM map centre, `docs/STALINGRAD_CAMPAIGN.md:11`), with one crewman as the
observer. I read the repository read-only at branch `claude/sound-overhaul-20260928`, commit
`c609667e3d73e1a88fd74da61a68b05fa7832410`. Nothing in the repository was changed; this file is the
only output. The report supports three items the owner approved on 28 Sep:
1. light in the air (lamps, fires and muzzle flashes scattering in falling snow and haze, with real
   shadows);
2. a real night, lit by burning buildings, flares, tracers, the Moon and the night sky;
3. physiological eye adaptation in place of the fixed exposure.

## How to read this

**Tags** (same convention as `sound_levels.md` in this folder)
- **[M]** measured, with citation.
- **[S]** standard, definition or published model from a recognised authority, fetched and read this session.
- **[R]** a standard or published value recalled from the literature. The original could not be fetched
  this session (paywall or 403). Its constants must be checked against the original before shipping.
  Where possible, a check I did run is stated next to it.
- **[D]** derived by me from a published law. The formula and inputs are shown and can be reproduced.
- **[E]** estimate: scaling or judgement from anchors, with the reasoning shown.
- **[A]** doctrine or eyewitness account (qualitative).
- **[C]** code fact read in the repository at c609667, cited as `file:line`.

**Units**
- **RU (renderer unit):** the physical backend's HDR value, a spectral radiance in W m⁻² sr⁻¹ nm⁻¹
  sampled at 680 / 550 / 440 nm (R, G, B lanes). §1.1 shows why.
- A spectrally flat 1 RU has a luminance of 72,983 cd/m² [D].
- `cp` (candlepower) is taken as 1 cd; the historical international candle is within 2 % of it.
- A troland (td) is L [cd/m²] × pupil area [mm²].

**Clocks**, all times are UT unless marked (§3.2):
- German Army clocks (CET) = UT+1;
- Moscow time (MSK) = UT+3;
- Stalingrad civil time = UT+4;
- local apparent solar time (LAT) ≈ UT+2 h 58 min, within about 2 min of MSK in December.

**Research process**
- The session's WebSearch budget was exhausted before this task began (`WebSearch` answered
  "200 of 200"). No general-purpose search engine was queried afterwards.
- Every source below was fetched directly by URL: NASA/Espenak phase catalogue, IANA tzdata, UCL CVRL (CIE
  tables), Bruneton's atmosphere code, a Meeus implementation (for the lunar series tables), USNO Circular
  171 and other DTIC reports through the Internet Archive, US War Department technical manuals, NIST FDS
  source, PMC, ACP, University of Washington (Warren), ING La Palma, AFIT, Frostbite and HDR-VDP papers.
- To locate known titles *inside* the Internet Archive and PubMed I used those archives' own catalogue
  queries.
- Paywalled or blocked sources are listed in §8.
- Numerical work (ephemeris, photometry, scattering integrals) was run as throw-away Python piped from
  stdin. No scratch files were written.

---

## 0. Findings

**1. Why the physical backend is black and the shader backend grey at the same camera.** It is a unit
error in one shared field, not a transport error.
- The workshop's authored lighting sets `camera_exposure: 1.25` (manifest `environment_lighting`, both
  `act1-winter-courtyard-20260925` and `act1-urban-intro-20260928`).
- `Renderer::apply_authored_lighting` copies it into `exposure_override` on every frame [C
  `src/renderer.rs:5480-5483`, `src/game.rs:14480-14481`].
- Both backends honour that override [C `src/renderer.rs:7002`].
- 1.25 is the shader backend's display-referred constant. The physical backend's own calibration is
  `DEFAULT_CAMERA_EXPOSURE = 320` [C `src/renderer.rs:750-754`]. The physical frame is therefore exposed
  **log2(320/1.25) = 8.00 stops** below its own calibration [D].
- The recorded gap (0.0008 against 0.051 mean linear, `docs/URBAN_INTRO_20260928.md:295-298`) inverts
  through ACES to a physical mean scene radiance of ≈0.0024 RU ≈ **150–175 cd/m²** [D, §7.6]. That is a
  normal, fully photopic overcast winter afternoon.
- An eye (or any meter keyed to 18 %) adapted to it would set exposure ≈ 0.18/0.0024 ≈ **75**. That is
  **5.9 stops** above 1.25: the "six stops".

**2. Which side is physically wrong.**
- The physical side's *radiance* is right to within a factor of about 1–3. At a 7.5° sun under
  stratiform overcast, USNO Circular 171 with Brown's cloud divisors predicts 730–2,440 lx, snow at
  210–700 cd/m² [S/D].
- The physical side's *display* is wrong: a stylized-unit constant is applied to physical units.
- The shader side is physically wrong in its radiometry:
  - its sky, sun and fill are unitless palettes normalised at a 31° sun;
  - its "night" palette is *brighter* than its own 7.5° overcast dusk (sky 0.0446 against 0.0263 in the
    G lane) [C/D];
  - its grey dusk is an authored outcome at a fixed 1.25, not a computed observer response.
- The fix is to make exposure an observer quantity in physical units. The physical scene must not be
  brightened.

**3. The authoring schema cannot express a physical exposure.**
- `LightingTuning::validate` accepts `camera_exposure` only in 0.02–8 [C
  `src/environment_authoring.rs:353`]. `from_live` clamps it to that range [C `:321`].
- The physical default of 320 is outside that range. Any authored lighting forces the physical path at
  least log2(320/8) = 5.3 stops dark [D].

**4. The current "night" is 16–22 stops too bright.**
- `DayPhase::Night` keeps 20 % of the Sun's irradiance (`night_factor` 0.80,
  `solar_irradiance *= 1 - night` at `src/renderer.rs:789-799, 6936`). The Sun shines from 35.4°
  (`light_dir` `(-0.24, 0.58, -0.78)`, `:809-816`) [C].
- USNO 171 gives 62,156 lx clear at 35.4°. 20 % of that under stratus cloud (÷10) is 1,243 lx [D].
- A moonlit overcast night is 0.017–0.057 lx; a moonless overcast night about 2×10⁻⁴ lx [M/D].
- The ratio is 2×10⁴ to 6×10⁶, or 14.4–22.4 stops [D].
- The Night key's direction (NNW, 35° up) is impossible for the Moon at Stalingrad. The Moon's
  declination range was ±19.3° in 1942–43, so the highest it can stand in the north is −22° [D].
- The task's line hint "near 6591" points to the menu renderer at c609667. The live solar scaling is at
  `:6936`.

**5. The HDR target cannot hold a night in absolute units.**
- HDR is `Rgba16Float` [C `src/renderer.rs:424`]. binary16 has minimum normal 6.10×10⁻⁵ and minimum
  subnormal 5.96×10⁻⁸ [S].
- In RU, moonlit snow is 6.7×10⁻⁷ (a subnormal quantised in about 9 % steps). Starlit snow is 2×10⁻⁹
  and overcast moonless snow 9×10⁻¹⁰; both flush to **zero** (§2.7) [D].
- A pre-exposed HDR (radiance × current exposure) is a numerical requirement for any physical night. It
  is neither a physics shortcut nor a cheat.

**6. The existing eye meter cannot adapt below dim daylight.**
- `exposure.wgsl` clamps metered luminance at `METER_MIN_LUMINANCE = 0.0005` RU and exposure at 360
  [C `shaders/exposure.wgsl:26-32, 97-101`]. 0.0005 RU is about 36 cd/m² [D].
- It meters Rec.709 weights applied to spectral samples [C `:45`]. For firelight spectra this
  over-reads luminance by up to 2.3× (§1.2).
- Its time constants, 0.22 s brightening and 1.8 s darkening [C `:121`], are a fast neural phase only.
  Physiological dark adaptation takes minutes (cones) to 30–40 min (rods) [M, §6.5].

**7. The hand-set light powers are 10²–10³ times too bright once read in the physical units they
are rendered in.**
- Read as W sr⁻¹ nm⁻¹ triplets, which the physical combine does (`ci.rgb * ci.w`,
  `shaders/combine.wgsl:743-757, 836-839`), the values are:

| Light | Code | Reads as | Physical estimate | Ratio |
|---|---|---|---|---|
| Own muzzle flash | 420 × (1, .74, .40) [C `src/game.rs:16945`] | 2.3×10⁷ cd | 2×10⁴–1×10⁵ cd [D/E, §4.6] | 230–1,100× |
| Burning wreck | 48 × (1, .30, .08) [C `:17034`] | 1.4×10⁶ cd | 5×10³–3×10⁴ cd | ~50–300× |
| Tableau pressure lamp | 5 at 2,800 K | 1.7×10⁵ cd | 300–500 cd | ~400× |
| Tableau stove | 2.5 at 1,850 K | 6.0×10⁴ cd | 2–70 cd | ~10³–10⁴× |

- The ledger already names this "calibration debt"; §4.9 gives calibrated replacements.

**8. Sun and Moon directions do not come from an ephemeris.** In engine axes +X = east, +Z = south
(`src/terrain.rs:4`):
- The courtyard's authored sun (elevation 7.5°, engine azimuth 38°) is at compass **128°** (SE). That is a
  sunrise direction.
- At 15:00 LAT on 21 Dec 1942 the Sun stands at 7.41°, compass **220.9°** (engine azimuth 130.9°). The
  authored azimuth is about 93° wrong [D].
- `DayPhase::Day` lights Act I from 31.5° elevation. The Sun never rises above 17.85° (solstice) to
  25.2° (5 Feb) in the campaign window [D].

**9. The Moon on the campaign's dates** [M: Espenak, reproduced to the minute; D for altitudes and lux]:
- Full moons: 22 Nov 1942 20:24, 22 Dec 1942 15:03, 21 Jan 1943 10:48 UT. New moons: 8 Dec 01:59,
  6 Jan 12:38, 4 Feb 23:29 UT.
- A winter full moon at Stalingrad culminates near 60°. On a clear night it gives **0.14–0.22 lx**
  (USNO 171).
- The dark (moonless) nights were about 4–11 Dec, 2–9 Jan and 31 Jan–7 Feb.
- On the Christmas Eve midnight flare ring (`docs/ENVSTORY.md:249`, 23:00 UT, CET midnight) a 96 %
  Moon stood about 59° up in the south (§3.5).

**10. What is needed for the approved work** (§7.9):
- pre-exposed HDR;
- a photometric meter;
- Sun and Moon from an ephemeris of date and time;
- the Moon and night-sky emission as modelled emitters;
- calibrated compact and area emitters (Planck spectra, photometric anchors);
- coupling of local firelight to the cloud base and the snow;
- an observer (eye) model that is labelled a reproduction transform, like the audio chain's;
- replacement of the test that bans adaptation
  (`camera_exposure_and_surface_energy_have_no_campaign_or_view_fill`, `src/renderer.rs:11047-11069`)
  with the gate list in §7.9.

---

## 1. Units, conversions and precision

### 1.1 What the physical backend's numbers mean [C/S]

- `AtmosphereParameters::default()` declares:
  - Rayleigh coefficients (5.802, 13.558, 33.100)×10⁻⁶ m⁻¹;
  - solar irradiance (1.474, 1.8504, 1.91198) in "the same three spectral bands"
    [C `src/physical_environment.rs:14-44, 75-100`].
- These are exactly the values of E. Bruneton's precomputed-atmosphere demo at λ = 680, 550 and 440 nm:
  - `kLambdaR/G/B = 680/550/440` in `model.h`;
  - `kSolarIrradiance[32]=1.474`, `[19]=1.8504`, `[8]=1.91198` in `demo.cc`;
  - `kRayleigh·λ⁻⁴ = 1.24062e-6·λ[µm]⁻⁴`.
- Bruneton's `model.h` states the units: "spectral radiance and irradiance values (in W.m⁻².sr⁻¹.nm⁻¹ and
  W.m⁻².nm⁻¹), at the 3 wavelengths kLambdaR, kLambdaG, kLambdaB" [S].
- So the physical HDR holds **spectral radiance per nm at three wavelengths**, displayed as if linear
  sRGB. This is a colorimetric shortcut: no spectral-to-XYZ step. It is outside this report's scope,
  but it matters for the meter (§1.2).
- Solar TOA illuminance from the full Bruneton table (360–830 nm), integrated against CIE V(λ):
  **132,538 lx** [D]. Expected about 128–135 klx.

### 1.2 Photopic and scotopic luminance from the three samples [D]

- Reconstruct the spectrum piecewise-linearly through the 440/550/680 samples, flat beyond the ends.
- Integrate against CIE 1924 V(λ) (K_m = 683 lm/W) and CIE 1951 V′(λ) (K′_m = 1700 lm/W). The 1-nm
  tables are CVRL `vl1924e_1.csv` and `scvle_1.csv` [S]. The results:

```
L_v [cd/m²]            = 12458.6·R680 + 52558.9·G550 +  7965.8·B440     (sum 72983)
L_v′ [scotopic cd/m²]  =  2322.5·R680 + 89437.1·G550 + 73261.6·B440     (sum 165021)
```

- **Validation:** the solar spectrum reads 0.987 of its true integral; a 5,800 K blackbody 0.988; 2,800 K
  1.011.
- **Firelight caveat:** steep low-temperature spectra are poorly represented by 3 linear samples. They
  read high: 1.42× at 1,500 K, 1.70× at 1,300 K, 1.92× at 1,200 K.
  - The current meter's Rec.709 weights on the samples read 1.66×, 2.02× and 2.31× high.
  - Emitters should therefore be *specified* by their photometric integral (§4.10), not by Planck point
    samples.
  - The meter should use these weights. Better, it should use a fire-aware band model.

### 1.3 Blackbody photometry [D]

This anchors every flame, flash and flare below.
- Planck spectral radiance integrated against V and V′.
- Check: at the platinum freezing point, 2041.4 K, the integral gives **5.97×10⁵ cd/m²**, against the
  historical candela definition of 60 cd/cm² = 6.0×10⁵ cd/m² (0.5 % agreement).

| T (K) | L_v (cd/m²) | L_v′ (scot cd/m²) | S/P | Lum. efficacy of radiation (lm/W) | B at 680 / 550 / 440 nm (RU) |
|---|---|---|---|---|---|
| 900 | 0.200 | 0.040 | 0.20 | 1.7×10⁻⁵ | 5.1×10⁻⁵ / 5.6×10⁻⁷ / 1.2×10⁻⁹ |
| 1000 | 2.69 | 0.71 | 0.26 | 1.5×10⁻⁴ | 5.3×10⁻⁴ / 1.0×10⁻⁵ / 4.5×10⁻⁸ |
| 1100 | 23.0 | 7.6 | 0.33 | 8.7×10⁻⁴ | 3.6×10⁻³ / 1.1×10⁻⁴ / 8.9×10⁻⁷ |
| 1200 | 139 | 55.6 | 0.40 | 0.0037 | 0.018 / 8.1×10⁻⁴ / 1.1×10⁻⁵ |
| 1273 (1000 °C) | 438 | 197 | 0.45 | 0.0092 | 0.050 / 0.0028 / 5.1×10⁻⁵ |
| 1300 | 648 | 304 | 0.47 | 0.013 | 0.070 / 0.0043 / 8.6×10⁻⁵ |
| 1400 | 2,438 | 1,316 | 0.54 | 0.035 | 0.224 / 0.018 / 5.2×10⁻⁴ |
| 1500 | 7,740 | 4,720 | 0.61 | 0.085 | 0.613 / 0.063 / 0.0025 |
| 1600 | 21,380 | 14,520 | 0.68 | 0.18 | 1.48 / 0.188 / 0.0096 |
| 1800 | 117,500 | 95,620 | 0.81 | 0.62 | 6.43 / 1.15 / 0.093 |
| 2000 | 463,800 | 437,100 | 0.94 | 1.61 | 20.8 / 4.94 / 0.573 |
| 2500 | 5.62×10⁶ | 6.93×10⁶ | 1.23 | 8.0 | 173 / 67.6 / 15.1 |
| 2800 | 1.66×10⁷ | 2.29×10⁷ | 1.39 | 14.9 | 428 / 207 / 61.2 |
| 3000 | 3.02×10⁷ | 4.47×10⁷ | 1.48 | 20.7 | 709 / 387 / 133 |

Between 1,100 and 1,500 K luminance grows roughly as T^19 (a factor of 337 for a 36 % temperature rise).
The effective visible temperature of a flame therefore decides its brightness far more than its size
does.

### 1.4 Other conversions

- **EV100:** EV100 = log2(L·100/12.5), so L = 2^(EV100−3) cd/m² for K = 12.5, the reflected-light meter
  constant [S: Lagarde & de Rousiers, *Moving Frostbite to PBR* v3.2, eq. 12].
- **Stellar magnitude to illuminance:**
  - V = 0 corresponds to 2.54×10⁻⁶ lx outside the atmosphere [R: Allen, *Astrophysical Quantities*].
  - Check: V_sun = −26.74 gives 1.26×10⁵ lx, against 1.33×10⁵ lx from the band table.
  - The Moon's formula below uses the equivalent "16.57 mag = 1 foot-candle" [R: Krisciunas & Schaefer
    1991]. It is consistent to 0.3 %.
- **Surface brightness:** V = 21.9 mag/arcsec² = 1.9×10⁻⁴ cd/m² (59 nL). Band zero points K = 18.1,
  42.6, 36.4, 30.8, 25.5 µJy/arcsec² at mag 20 for U, B, V, R, I [S: Benn & Ellison 1998, appendix,
  citing Bessell 1979]. 1 arcsec² = 2.3504×10⁻¹¹ sr.
- **binary16 (IEEE 754) [S]:** max 65,504; min normal 6.104×10⁻⁵; min subnormal 5.96×10⁻⁸; relative
  step 4.9×10⁻⁴.
  - In RU the normal floor is 6.104×10⁻⁵ × 72,983 = **4.5 cd/m²**; below it precision degrades.
  - The subnormal floor is **0.0044 cd/m²**; below it the value is zero [D].

---

## 2. Natural light at night and in twilight (deliverable 1)

### 2.1 Sun and twilight illuminance against solar altitude [S]

**Source.** US Naval Observatory Circular 171, Janiczek & DeYoung, *Computer programs for Sun and Moon
illuminance* (1987), DTIC ADA182110. I read it via the Internet Archive and took the formula from its
FORTRAN and BASIC listings.
- It fits the measured "basic curve" of D. R. E. Brown, *Natural Illumination Charts* (US Navy BuShips
  374-1, 1952). That curve rests on more than 12,000 measurements made in the Arctic, the Antarctic and
  temperate and torrid zones between January 1943 and May 1947 [M].
- It also fits Jones & Condit (JOSA 38, 1948).

**Formula**, for the Sun (and, with the phase term, the Moon):
```
HA = h + 1/tan((h + 8.6/(h+4.42))°)/60           (refraction; skipped when h < −5/6°)
U  = sin HA;  X = 753.66156;  S = asin(X·cos HA /(X+1))
m  = X·(cos S − U) + cos S                        (air mass, homogeneous spherical atmosphere)
M  = e^(−0.21 m)·U + 0.0289·e^(−0.042 m)·(1 + (HA+90)·U/57.29578)
E_sun = 133775·M / SK   [lx, horizontal]
```

**Sky-condition divisor SK** (Brown's criteria, quoted in the Circular) [M]:
- 1: average clear sky, under 7/10 cover, direct rays unobstructed;
- 2: sun or moon visible through thin cloud;
- 3: average cloud obstructing the rays;
- 10: dark stratus covering the sky ("not common").

**Night-sky term:** the programs add 0.0005/SK lx. The text says 0.0003 lx.

**Validity:** the fit runs from the zenith to nautical twilight. The text quotes a range of 124,000 lx
to 0.0003 lx.

| Solar altitude | E clear (lx) | ÷3 average cloud | ÷10 dark stratus |
|---|---|---|---|
| 60° | 102,960 | 34,320 | 10,300 |
| 35.4° (current Night key) | 62,156 | 20,720 | 6,216 |
| 31° (stylized reference) | 53,359 | 17,790 | 5,336 |
| 25.2° (noon, 5 Feb 1943) | 41,300 | 13,800 | 4,130 |
| 17.85° (noon, solstice) | 26,182 | 8,727 | 2,618 |
| 10° | 11,231 | 3,744 | 1,123 |
| **7.5° (courtyard / 15:00 LAT)** | **7,329** | **2,443** | **733** |
| 5° | 4,252 | 1,417 | 425 |
| 0° | 984 | 328 | 98 |
| −0.83° (sunset) | 662 | 221 | 66 |
| −3° | 66.4 | 22.1 | 6.6 |
| **−6° (civil dusk)** | **2.99** | 1.00 | 0.30 |
| −9° | 0.116 | 0.039 | 0.012 |
| **−12° (nautical dusk)** | **0.0044** (+0.0005 sky) | 0.0015 | 0.0004 |
| −15° | 1.7×10⁻⁴ (+0.0005 sky) | — | — |

**Values commonly quoted in the literature.** 3.4 lx at −6°, 0.008 lx at −12° and 6.6×10⁻⁴ lx at −18°
(Brown/Bond & Henderson lineage) [R]. They agree with the Circular within about 15 % at −6° and a factor
of 2 at −12°.

**Twilight colour** [M: Spitschan, Aguirre, Brainard & Sweeney 2016, *Sci Rep* 6:26756, rural Cherry
Springs]:
- downwelling light is enriched in short wavelengths from daylight through nautical twilight;
- chromaticity is bluest near −12°, then reddens (Chappuis ozone filtering);
- the moonless night spectrum shows the 558 nm atomic-oxygen airglow line.

The physical atmosphere already carries ozone absorption (0.650, 1.881, 0.085)×10⁻⁶ m⁻¹ [C
`src/physical_environment.rs:85`], so the blue hour can emerge from transport. Single scattering alone
under-predicts deep twilight; multiple scattering dominates below about −6°. The ledger lists Rayleigh
multiple scattering as missing (`docs/PHYSICAL_TRUTH_AUDIT.md:55`). The rows above are the validation
targets.

### 2.2 Sky luminance in twilight and under overcast [D]

- For a sky close to uniform, E_h = π·L_sky, so L_sky ≈ E/π.
- **Civil dusk:** L_sky ≈ 0.95 cd/m² clear; about 0.1 cd/m² under stratus.
- **Nautical dusk:** about 1.6×10⁻³ cd/m².
- **Overcast afternoon at 7.5°:** 233–778 cd/m² (÷10 to ÷3).
- For comparison, a physically reconstructed daytime HDR sky reads about 8,000 cd/m² clear and about
  2,000 cd/m² overcast [S: Frostbite v3.2 notes].

### 2.3 Moonlight

**Measured** [M: Kyba, Mohar & Posch 2017, "How bright is moonlight?", *Astronomy & Geophysics*
58(1):1.31]:
- maximum horizontal illuminance about 0.3 lx: a zenith, perigee, clear "supermoon";
- measured on 14 Nov 2016 near Vienna: 0.26 lx horizontal, 0.30 lx facing the Moon;
- "most full moons at mid-latitudes produce only 0.05–0.2 lx";
- apogee against perigee: nearly 20 % difference;
- E ∝ sin(elevation);
- published claims of 0.5–2.2 lx are errors.

**Model** [S: USNO Circular 171, as §2.1]:
```
H  = h − 0.95°·cos h          (topocentric parallax, mean)
E  = elongation of the Moon from the Sun (rad)
P  = 0.892·exp(−3.343 / tan(E/2)^0.632) + 0.0344·(sin E − E cos E)       (Lumme–Bowell phase law)
P  = 0.418·P / (1 − 0.005 cos E − 0.03 sin h)
E_moon = P·M(H)/SK     [lx]      (M as for the Sun)
```

**Cross-check** [R: Krisciunas & Schaefer 1991, PASP 103:1033]:
- V(α) = −12.73 + 0.026|α| + 4×10⁻⁹α⁴, with α the phase angle in degrees, at mean distance;
- E_TOA = 10^(−0.4(V+16.57)) foot-candles;
- extinction k_V = 0.25 mag per air mass [E].
- On the campaign nights the two models agree within 5–15 % (§3.5).

**Full moon at mean distance, α ≈ 2°:**
- E_TOA = 0.30 lx (K&S) to 0.33 lx (USNO) [D].
- Disc solid angle: π·(1737.4/384,400)² = 6.42×10⁻⁵ sr.
- Mean disc luminance: **4,650–5,190 cd/m²** [D]. Frostbite quotes "around 2,500 cd/m²" without a
  derivation.

**In renderer units:**
- Assume the solar spectrum times a lunar reflectance rising with wavelength, relative 0.80 / 1.00 / 1.22
  at 440 / 550 / 680 nm [E].
- The full Moon's disc radiance is then **(R, G, B) = (0.067, 0.069, 0.057) RU** [D, scaled to
  4,900 cd/m² with §1.2].
- Moon/Sun illuminance ratio at full: 2.3×10⁻⁶ [D].

**Phase dependence** (USNO phase law, TOA, relative to full): quarter moon (α = 90°) ≈ 8 % of full [D].
- With the Moon at 40° under a clear sky: E ≈ 0.015 lx.
- The classic decade table's "quarter moon 0.01 lx, full moon 0.1 lx" is consistent.

**Moonlit sky** [D]:
- Sky radiance is linear in source strength.
- So a moonlit clear sky ≈ day-sky luminance at the same geometry × 2.3×10⁻⁶ ≈ 8,000 × 2.3×10⁻⁶ ≈
  **0.018 cd/m²** (about 17.6 mag/arcsec² in V) at full moon.
- That is 100× the natural moonless sky.
- The physical atmosphere will produce this itself once the Moon is an emitter.

### 2.4 The moonless clear night sky

**Measured** [M: Benn & Ellison 1998, *La Palma night-sky brightness*, ING; 427 CCD images, 63 nights]:
- median dark-of-moon zenith, sunspot minimum, high ecliptic and galactic latitude:
  **U ≈ 22.0, B = 22.7, V = 21.9, R = 21.0, I ≈ 20.0 mag/arcsec²**;
- brighter by 0.4 mag at low ecliptic latitude;
- brighter by 0.4–0.5 mag at solar maximum;
- brighter by 0.25 mag at air mass 1.5;
- airglow : zodiacal light ≈ 2.5 : 1;
- airglow is redder than sunlight longward of 450 nm.

**Standard reference** [S: Falchi et al. 2016, *Sci Adv* 2:e1600377]: "22.0 mag/arcsec², corresponding
to 174 µcd/m², as a typical brightness of the night sky background during solar minimum".

**Solar activity in 1942–43** [E]: the declining phase of cycle 17, minimum about 1944. Take the
sunspot-minimum values, perhaps 0.1–0.2 mag brighter.

**Renderer triplet** [D]:
- Convert B, V, R with the Bessell zero points of §1.4, and F_λ = F_ν·c/λ².
- Zenith radiance **(R680, G550, B440) ≈ (3.4, 2.7, 2.3)×10⁻⁹ RU**.
- §1.2 then gives 2.0×10⁻⁴ cd/m² (check against 1.9×10⁻⁴ ✓) and 4.2×10⁻⁴ scotopic cd/m², so
  **S/P ≈ 2.1**.
- Brightening toward the horizon of a thin airglow layer at about 90 km follows van Rhijn:
  V(z) = 1/√(1 − (R/(R+h))²·sin²z). This is ×4.2 at z = 80° and ×6.0 at the horizon, before extinction
  [S].

**Horizontal illuminance** [M: Jechow et al. 2017, *Sci Rep* 7:6741]:
- π × 0.25 mcd/m² ≈ **0.78 mlx** ideal;
- hemispherical scalar 1.56 mlx.
- USNO 171 uses 0.3–0.5 mlx. Spitschan et al. quote "starlight ~0.001 lx".
- Take **0.5–0.8 mlx** clear and moonless, including stars.

### 2.5 Overcast nights, natural and fire-lit

**Natural** [M: Jechow & Hölker 2019, *J Imaging* 5:69, PMC8320966]:
- the darkest zenith luminance they measured was **0.07 mcd/m², under overcast at a very remote site**.
  In natural settings clouds darken the sky; they amplify it only where there is artificial light.
- Horizontal illuminance of such a night: π × 0.07 mcd/m² ≈ **2×10⁻⁴ lx** [D].
- USNO's night term with Brown's divisor 10 gives 3–5×10⁻⁵ lx [S].
- An overcast moonless steppe night is therefore **3×10⁻⁵ – 2×10⁻⁴ lx**, with snow on the ground. That is
  close to the absolute threshold of useful vision (§6.1).

**With local light, snow and cloud** ("snowglow") [M, same paper]:

| Site | Clear, no snow | Overcast, no snow | Overcast with snow |
|---|---|---|---|
| Suburban (zenith) | 1.3 mcd/m², E_h 7.0 mlx | 7.3 mcd/m², E_h 22 mlx | **244 mcd/m², E_h 0.79 lx** |
| Rural Finland, near the Arctic Circle | 0.5 mcd/m², E_h 2.2 mlx | — | 13 mcd/m², E_h 34 mlx |

- Suburban with snow and cloud is ×188 the clear, snow-free value and ×33 the overcast, snow-free value.
- Rural Finland rises ×26 from clear to overcast with snow.
- Stalingrad had no electric light (blackout), but it had fires, flares and gun flashes. The same
  physics applies to them.

**Fire-glow under overcast** [D/E]. Model the ground and the cloud base as parallel diffuse reflectors:
- sources of total luminous flux Φ in an area S;
- snow albedo A;
- cloud-base reflectance R_c. Delta-Eddington with τ ≈ 48 and g = 0.85 gives about 0.84 for ω = 1; take
  0.8.
```
F_up   = Φ·(1+A)/2 · T_smoke / S
E_down = R_c·F_up / (1 − R_c·A)          (inter-reflection gain 1/(1−0.64) = 2.8)
```
- Example: 20 fully involved buildings at 5×10⁴ cd each (§4.4) in a 4 km-radius effective area, with
  T_smoke = 0.5. This gives E_down ≈ **0.25 lx** on the snow, and a cloud-base luminance ≈ 0.08 cd/m².
- 5 fires give ≈ 0.06 lx.
- Range: **0.05–0.5 lx**, comparable to or brighter than a clear full Moon. This is the physics behind
  the red-grey glow over a burning city at night.
- It exists only if the renderer carries positioned-emitter light up to the cloud base and back (§7.9).
  The ledger lists "positioned-emitter scattering and snow/atmosphere coupling" as unresolved
  (`docs/PHYSICAL_TRUTH_AUDIT.md:54`).

**Doctrine** [A: *Night Combat*, DA Pamphlet 20-236 (1953), written by former German officers of the
Eastern Front]:
- "Operations taking place during moonlight and starlit nights, especially across snow-covered terrain,
  may approximate daytime conditions."
- "Very hazy, rainy, foggy, or overcast weather calls for reliance on the auditory rather than on the
  visual sense."
- Orientation aids named: "illumination of enemy terrain by artillery fire on inflammable targets, fires
  lighted behind one's own MLR, Very lights, parachute flares, searchlights, machine gun tracer fire".
- The trainee "can perceive only the outline of an object without any detail" and "observe[s] better
  from below than from above".

### 2.6 Snow albedo

**Measured model** [M: Dang, Brandt & Warren 2015, JGR 120, doi:10.1002/2014JD022646, Table 1] for pure
deep snow of grain radius 100 µm:

| Band | Clear sky | Overcast |
|---|---|---|
| Visible, 0.3–0.7 µm | **0.9849** | **0.9856** |
| All-wave, 0.3–4 µm | 0.834 | 0.899 |

- Coarsening from 100 to 1,000 µm lowers all-wave albedo by 12 %.
- Black carbon at 20 ng/g lowers it by 0.5 % at 100 µm and 1.6 % at 1,000 µm.

**Ice absorption** [M: Warren & Brandt 2008, UW `IOP_2008_ASCIItable.dat`]: imaginary index k and
absorption coefficient γ = 4πk/λ:

| λ | k | γ |
|---|---|---|
| 440 nm | 6.27×10⁻¹¹ | 0.0018 m⁻¹ |
| 550 nm | 2.29×10⁻⁹ | 0.0523 m⁻¹ |
| 680 nm | 2.09×10⁻⁸ | 0.386 m⁻¹ |

**Spectral snowpack albedo** [D]:
- Asymptotic radiative transfer for weak absorption: A = exp(−4·√((1−ω)/(3(1−g)))). [R: Kokhanovsky &
  Zege 2004, *Appl Opt* 43:1589; g = 0.89.]
- Grain: 1−ω_ice = (2/3)·B·γ·r with B = 1.5 [R].
- Soot: 1−ω_BC = C·MAC·(4ρ_ice·r)/(3·Q_ext), with MAC = 7.5 m²/g at 550 nm [R: Bond & Bergstrom 2006]
  and Q_ext = 2.
- **Check:** r = 100 µm and C = 0 gives A(550) = 0.984, against Dang's 0.985 ✓.

| Grain radius | Soot (ng/g) | A 440 | A 550 | A 680 | Reading |
|---|---|---|---|---|---|
| 50 µm | 0 | 0.998 | 0.989 | 0.970 | new powder |
| 100 µm | 0 | 0.997 | 0.984 | 0.958 | fresh snow |
| 250 µm | 0 | 0.995 | 0.975 | 0.934 | days old |
| 500 µm | 0 | 0.993 | 0.965 | 0.908 | old, wind-packed |
| 250 µm | 100 | 0.928 | 0.924 | 0.904 | lightly sooted |
| 250 µm | 1,000 | 0.790 | 0.789 | 0.782 | near fires and roads |
| 500 µm | 10,000 | 0.348 | 0.348 | 0.347 | trampled, burnt ground |

- Pure snow is slightly blue: red is absorbed after many scatterings.
- Soot flattens the spectrum and darkens it.
- Near burnt buildings, gun positions and tracks, use 0.35–0.8 [E]. Use 0.95–0.98 for fresh snow away
  from the fighting (visible band).
- Snow is forward-scattering (non-Lambertian). A Lambertian albedo is adequate for night levels; the BRDF
  matters for specular sheen at grazing sun angles.

### 2.7 Snow luminance ladder for the campaign [D]

Assumptions: snow albedo 0.9 in the visible; age 25 for the pupil; §6.3 and §6.1 for the regime.

| Scene | E (lx) | Snow L (cd/m²) | EV100 | RU (Y) | binary16 | Pupil (mm) | CIE 191 m | Regime |
|---|---|---|---|---|---|---|---|---|
| Overcast noon, mid-Jan (20°, stratus ÷10) | 3,060 | 877 | 12.8 | 0.012 | normal | 2.4 | 1.00 | photopic |
| 7.5° dusk, stratus ÷10 (courtyard) | 733 | 210 | 10.7 | 0.0029 | normal | 2.7 | 1.00 | photopic |
| Civil dusk (−6°), clear | 2.99 | 0.86 | 2.8 | 1.2×10⁻⁵ | subnormal | 5.3 | 0.77 | mesopic |
| Civil dusk (−6°), stratus | 0.30 | 0.086 | −0.6 | 1.2×10⁻⁶ | subnormal | 6.5 | 0.46 | mesopic |
| Under an LC 50 flare, 300 m overhead | 6.67 | 1.91 | 3.9 | 2.6×10⁻⁵ | subnormal | 4.8 | 0.86 | mesopic |
| 50 m from a burning house (5×10⁴ cd) | 20 | 5.7 | 5.5 | 7.9×10⁻⁵ | normal | 4.2 | 1.00 | photopic |
| 200 m from a burning house | 1.25 | 0.36 | 1.5 | 4.9×10⁻⁶ | subnormal | 5.8 | 0.60 | mesopic |
| Full moon at 60°, clear | 0.17 | 0.049 | −1.4 | 6.7×10⁻⁷ | subnormal | 6.8 | 0.39 | mesopic |
| Full moon, average cloud (÷3) | 0.057 | 0.016 | −2.9 | 2.2×10⁻⁷ | subnormal | 7.1 | 0.25 | mesopic |
| Full moon, stratus (÷10) | 0.017 | 0.0049 | −4.7 | 6.7×10⁻⁸ | subnormal | 7.4 | 0.10 | scotopic |
| Fire-glow under overcast (§2.5) | 0.15 | 0.043 | −1.5 | 5.9×10⁻⁷ | subnormal | 6.8 | 0.27 | mesopic |
| Quarter moon at 40°, clear | 0.015 | 0.0043 | −4.9 | 5.9×10⁻⁸ | **zero** | 7.5 | 0.08 | scotopic |
| Clear, moonless | 0.0005–0.0008 | 1.4–2.2×10⁻⁴ | −9.8 | 2–3×10⁻⁹ | **zero** | 7.8 | 0 | scotopic |
| Overcast, moonless | 0.0002 | 6×10⁻⁵ | −11.0 | 9×10⁻¹⁰ | **zero** | 7.9 | 0 | scotopic |

- The ladder spans about 24 stops.
- Everything at night sits 11–24 stops below the physical backend's fixed calibration (EV100 8.4 at 320).
- Everything below about civil dusk is subnormal or zero in the current HDR format.

---

## 3. Ephemeris for Stalingrad, Nov 1942 – Feb 1943 (deliverable 2)

### 3.1 Method and validation [D]

**Sun:** Meeus, *Astronomical Algorithms*, ch. 25 (low-accuracy, about 0.01°) with the apparent-longitude
correction.
- Validated on Meeus example 25.a (1992 Oct 13.0 TD): λ_app 199.90894° (book 199.90895), RA 198.38083°,
  dec −7.78507°, R 0.99766 — all equal.

**Moon:** Meeus ch. 47, the full 60+60-term series. Coefficients were parsed from the MIT-licensed
`soniakeys/meeus` Go implementation.
- Validated on example 47.a (1992 Apr 12.0 TD): λ 133.162655°, β −3.229126°, Δ 368,409.7 km — all equal.
  RA 134.688438° against 134.688470°: the difference is my truncated nutation (0.1″).

**Other steps:**
- ΔT = 26 s (1943) [R: Espenak & Meeus polynomial];
- GMST from Meeus 12.4;
- topocentric lunar altitude h − HP·cos h;
- rise and set at h₀ = −0.8333° (Sun) and 0.7275·HP − 0.5667° (Moon) [S: Meeus ch. 15];
- twilights at −6°, −12° and −18°.

**Phases:** computed as roots of elongation 0/90/180/270°. All 17 phases from Nov 1942 to Feb 1943 match
the NASA/Espenak *Six Millennium Catalog* to the minute.

### 3.2 Clocks in the pocket [S: IANA tzdata `europe`, fetched]

- **German:** `Rule C-Eur 1942 Nov 2 2:00s → standard` and `1943 Mar 29 → summer`. German clocks ran
  CET = **UT+1** throughout Dec 1942 – Feb 1943.
- **Moscow:** `Zone Europe/Moscow … 3:00 Russia MSK/MSD` from 1930, so MSK = **UT+3**.
- **Stalingrad civil:** `Zone Europe/Volgograd … 4:00 +04 1930 Jun 21 → 1961 Nov 11`, so **UT+4**.
- In German time the midwinter Sun rises about **05:52** and sets about **14:06–14:15**. Civil twilight
  ends about **14:45** and it is fully dark (astronomical) by about **16:05**.
- "15:00" in German time is already −8° (nautical twilight).
- The authored "15:00 dusk" at 7.5° is 15:00 **local solar/Moscow** time. That is 13:00 German time
  (§3.3).

### 3.3 Sun summary [D] (full daily table in Appendix A)

| Date | Rise–set (UT) | Noon alt. | Day length | Civil / naut. / astro. dusk (UT) | Rise / set azimuth |
|---|---|---|---|---|---|
| 19 Nov 1942 | 04:15–13:19 | 21.94° | 9.05 h | 13:53 / 14:32 / 15:09 | 119° / 241° |
| 21 Dec 1942 (solstice) | 04:52–13:08 | **17.85°** | **8.27 h** | 13:45 / 14:25 / 15:04 | 126° / 234° |
| 15 Jan 1943 | 04:50–13:32 | 20.04° | 8.70 h | 14:08 / 14:47 / 15:25 | 122° / 238° |
| 2 Feb 1943 | 04:32–14:00 | 24.28° | 9.47 h | 14:34 / 15:12 / 15:49 | 115° / 245° |

- Solar noon falls at 08:47–09:16 UT (**12:00 MSK ≈ 10:00 CET**).
- **15:00 LAT on 21 Dec** is 12:00 UT: altitude 7.41°, compass azimuth 220.9°, engine azimuth 130.9°,
  engine direction (east, up, south) = (−0.649, +0.129, +0.750).
- On 15 Jan 1943 the afternoon Sun reaches 7.5° at 12:27 UT, azimuth 225.3° (engine 135.3°).

### 3.4 Moon phases [M: Espenak; reproduced, D]

| Phase | Nov 1942 | Dec 1942 | Jan 1943 | Feb 1943 |
|---|---|---|---|---|
| New | 8 Nov 15:19 | 8 Dec 01:59 | 6 Jan 12:38 | 4 Feb 23:29 (total solar eclipse, not visible here) |
| First quarter | 15 Nov 06:57 | 14 Dec 17:47 | 13 Jan 07:49 | 12 Feb 00:40 |
| Full | 22 Nov 20:24 | **22 Dec 15:03** | **21 Jan 10:48** | 20 Feb 05:45 (partial lunar eclipse, below the horizon here) |
| Last quarter | 1 Dec 01:37 | 30 Dec 18:37 | 29 Jan 08:13 | 27 Feb 18:23 |

- The 18.6-year node cycle was near a minor standstill in 1941–42. The Moon's declination therefore ran
  only ±19.3°. The winter full moon culminated at **58–60°**, due south near local midnight (≈21 UT).
- There is no northern Moon at any time [D].
- A full Moon in Dec–Jan is up for the whole dark period, about 16 h [D].

### 3.5 The campaign's dates [D] (Moon under a clear sky, USNO 171; moonlight under cloud: ÷3 average, ÷10 stratus)

| Date | Event (historical time [R]) | Sun (UT): rise / set / astro. dark | Moon at 21 UT | Brightest moonlight in the dark hours |
|---|---|---|---|---|
| 19 Nov 1942 | Op. Uranus opens (07:30 MSK = 04:30 UT, fog and snow) | 04:15 / 13:19 / 15:09 | 91 % waxing, sets 01:13 | 0.068 lx at 18:49 (46°); no Moon at the H-hour |
| 23 Nov 1942 | Pincers meet at Kalach/Sovetsky | 04:21 / 13:15 / 15:06 | 99 %, 58° | 0.159 lx at 21:29 |
| 12 Dec 1942 | Op. *Wintergewitter* starts | 04:45 / 13:06 / 15:01 | 30 % waxing, sets 18:26 | 0.003 lx (dusk only) |
| 16 Dec 1942 | Op. Little Saturn starts | 04:48 / 13:06 / 15:02 | 72 % waxing, sets 23:05 | 0.032 lx |
| 19 Dec 1942 | Relief reaches the Myshkova | 04:51 / 13:07 / 15:03 | 93 % waxing | 0.090 lx at 19:00 (56°) |
| 21–22 Dec 1942 | Solstice; full Moon | 04:52 / 13:08 / 15:04 | 99–100 %, 59–60° | **0.18–0.22 lx** |
| 24 Dec 1942 | Relief abandoned; Tatsinskaya | 04:53 / 13:09 / 15:05 | 96 % waning, rises 15:03 | 0.111 lx at 22:37 (59°) |
| 24→25 Dec, 23:00 UT (CET midnight) | Pocket-wide flare salute (`docs/ENVSTORY.md:249`) | — | 96 %, about 59° in the south | 0.11 lx if clear |
| 10 Jan 1943 | Op. *Kol'tso* opens (08:05 MSK = 05:05 UT, ≈ sunrise) | 04:53 / 13:26 / 15:20 | 25 % waxing, sets 18:33 | 0.002 lx |
| 16 Jan 1943 | Pitomnik lost | 04:50 / 13:34 / 15:26 | 82 % waxing | 0.058 lx |
| 22 Jan 1943 | Gumrak lost | 04:45 / 13:43 / 15:34 | 98 % waning | 0.137 lx |
| 26 Jan 1943 | Pocket split at Mamayev Kurgan | 04:41 / 13:49 / 15:39 | 75 % waning, rises 19:02 | 0.032 lx |
| 31 Jan 1943 | Paulus surrenders | 04:35 / 13:57 / 15:46 | 23 % waning, rises 00:50 | 0.001 lx (pre-dawn) |
| 2 Feb 1943 | Northern pocket surrenders | 04:32 / 14:00 / 15:49 | 7 %, not up in the dark | 0 |

The two lunar models agree within 5–15 % on these nights; for example 23 Nov gives 0.159 lx (USNO)
against 0.140 lx (K&S). The urban intro is set in January 1943 (`docs/URBAN_INTRO_20260928.md:16`).
Mid-January nights run from the waxing gibbous of 14–20 Jan to the full Moon of 21 Jan. 2–9 Jan and
31 Jan–5 Feb were moonless.

### 3.6 The current code against the ephemeris [C/D]

| Code | Sun or Moon direction | Physically possible in the window? |
|---|---|---|
| Courtyard / urban manifest `environment_lighting` (elev 7.5°, az 38°) | compass 128°, 7.5° | Only within minutes of sunrise, and even then the azimuth is 11° off (139° at 7.5° in the morning). Wrong by about 93° for the stated 15:00 dusk. |
| `DayPhase::Day` `(-0.58, 0.40, 0.30)` [C `src/renderer.rs:812`] | compass 243°, 31.5° | No. The winter maximum is 17.85–25.2°. |
| `DayPhase::Dusk` `(0.80, 0.08, -0.38)` [C `:813`] | compass 65°, 5.2° | No. The winter Sun sets at 234–246°. |
| `DayPhase::Night` `(-0.24, 0.58, -0.78)` [C `:814`] | compass 343°, 35.4° | No, for the Moon: its northern maximum is −22°. |

### 3.7 Runtime use [D]

- The game should hold a date and time (UT) and compute:
  - the Sun's and Moon's directions (§3.1 algorithms);
  - the Moon's phase angle and distance;
  - the Moon's disc radiance (§2.3).
- The algorithms fit in about 200 lines, need no data files beyond the 120-row lunar table, and run in
  microseconds.
- Validation vectors belong in a unit test: Meeus 25.a and 47.a, and the Espenak phase times of Dec 1942.

---

## 4. Light sources of the battle (deliverable 3)

### 4.1 Aircraft parachute flares

| Flare | Luminous intensity | Burn | Descent | Notes | Tag |
|---|---|---|---|---|---|
| German **FB 50** (single candle) | **216,000 cp** | "slightly longer than 5 minutes" | — | Al 16.5 %, Ba(NO₃)₂ 75.8 %, S 7.7 % (greenish-white Ba light) | [M: TM 9-1985-2 (1953), pp. 67–68] |
| German **LC 50 F Ausf. C** (four candles) | **600,000 cp** | > 5 min | 13-ft celanese parachute, 12 silk lines | total 76 lb | [M: TM 9-1985-2] |
| German single-candle white flare, pull-friction ignition | — | 5 min | 4-ft parachute (Ausf. E) | — | [M: TM 9-1985-2 p. 74] |
| German red flare in the same family | — | about 6 min | — | — | [M] |
| US **AN-M26** | 800,000 cp (575,000 alternative candle) | 3–3.5 min | **11.6 ft/s (3.5 m/s)** | released 1,200–25,000 ft above burst height | [M: TM 9-1981 (1943)] |
| US 350,000 cp flare | — | 3 min | 8 ft/s | — | [M: TM 9-1981] |
| US M9 flare | 60,000 cp | 1 min | 7 ft/s | — | [M: TM 9-1981] |
| Soviet SAB aircraft flares (Po-2 night bombers) | 2×10⁵–1×10⁶ cd | 3–6 min | 3–6 m/s | bracketed by the German and US families above; no Soviet photometry retrieved | [E] |

**LC 50 descent** [E]: for a candle unit of about 20 kg under a 13-ft (3.96 m) chute, with C_d 0.8 and
ρ 1.35 kg/m³ (−20 °C), v = √(2mg/(ρC_dA)) ≈ 5 m/s. It falls about 1.5 km during the burn, so release
must be at 1.5–2 km.

**Ground illuminance under a flare** [D]: E = I·H/(H²+x²)^1.5.

| Flare | At 0 m | At 200 m | At 400 m | At 800 m |
|---|---|---|---|---|
| LC 50, 600 kcd at H = 300 m | 6.7 lx | 3.8 lx | 1.4 lx | 0.29 lx |
| US 60 mm illuminating shell, 145 kcd at 250 m | 2.3 lx | 1.1 lx | 0.35 lx | 0.06 lx |

Snow directly under the LC 50 is at 1.9 cd/m², in the mesopic range.

### 4.2 Signal-pistol stars

- **German Leuchtpistole:** "26.7-mm caliber, has a smooth bore, and fires a series of colored signal
  cartridges, as well as a whistling cartridge" [M: TM-E 30-451 *Handbook on German Military Forces*,
  1945, §VII-3]. The rifled *Kampfpistole* and *Sturmpistole* variants also fire the standard
  cartridges.
- **Rifle-launched parachute star** (*Gewehr-Fallschirmleuchtgranate*): container ejected at about 6.5 s,
  parachute and star 2 s later [M: TM-E 30-451].
- **No German or Soviet photometry was found** for 26–27 mm white stars. German Wikipedia states only
  that signal cartridges burn "at a height of about 100 m", up to 300 m for modern types [S: de.wikipedia
  *Signalpistole*].
- **US analogues** [M: TM 9-1981, 1943]:
  - white star parachute M17: 20,000 cp for 20–30 s, falling 7 ft/s (2.1 m/s), fired to about 600 ft;
  - white star cluster M18: 5 × 18,000 cp for 5–7 s;
  - green parachute: 5,000 cp; amber parachute: 4,000 cp; red parachute: 20,000 cp.

**Estimate for a German or Soviet 26 mm white star without parachute** [E]:
- 1–2×10⁴ cd;
- 6–8 s burning on the rise and fall;
- apex 80–100 m.

Derivation: 10–15 g of Mg/nitrate star at 8,000–18,000 cd·s/g luminous efficiency [M: AMCP 706-185
(1967), §6 table of Mg/nitrate flare compositions, 8,000–18,000 candle-s/g, OCR-read] burning 6–8 s gives
1–4×10⁴ cd. Take the lower half for small-calibre stars. With a parachute (Soviet and German types
existed [E]): 20–40 s at 2 m/s.
- Ground illuminance from a 2×10⁴ cd star at 90 m: 2.5 lx below, 0.17 lx at 200 m, 0.026 lx at 400 m
  [D].
- Coloured stars are 4–5× weaker than white (US ratios, [M]).

### 4.3 Illuminating shells, trip flares, searchlights

- **US 60 mm illuminating shell:** ejects at 14 s; candle burns about 25 s at 145,000 cp (110,000 with
  the substitute composition), falling about 10 ft/s [M: TM 9-1981].
- **US trip flare (parachute):** thrown to 300–500 ft; burns about 20 s at 110,000 cp; "illuminates
  effectively a circle of 300 yards radius" [M]. Ground trip flare: 40,000 cp for about 1 min [M].
- **German artillery illuminating shells existed:** an artillery shell whose "parachute and
  illuminating container are base ejected" (the calibre entry is garbled in the OCR), and a heavy shell
  with a 47-lb flare-and-parachute assembly [M: TM 9-1985-3 (1953)]. No photometry was given. Estimate 10⁵–10⁶ cd for 25–60 s [E].
- **German Flak searchlights** [M: TM-E 30-451, 1945]:
  - 150 cm Scheinwerfer 34/37: **990 million cp**;
  - 200 cm: 2,430 million cp;
  - 60 cm: 135 million cp.
  - A 150 cm beam near an airfield is a real, historically attested emitter, e.g. at Pitomnik [E].

### 4.4 Burning buildings

**Temperatures:**
- Post-flashover compartment gas follows roughly the standard fire curve
  T = 20 + 345·log₁₀(8t+1) °C (t in min): 842 °C at 30 min, 945 °C at 60 min, 1,049 °C at 120 min
  [R: ISO 834-1].
- Window openings of a burning house therefore glow at **1,100–1,350 K**.
- Luminous sooty flames 1 m or more thick are close to blackbodies [R: Tien et al., SFPE Handbook;
  ε = 1−e^(−κL), κ ≈ 0.8 m⁻¹ for wood].

**Luminance:** from §1.3, 1,100 K → 23 cd/m², 1,200 K → 139, 1,300 K → 648, 1,400 K → 2,440.
- Time-averaged flames contain hotter streaks, and visible output weights the hottest part strongly.
- Take an **effective visible temperature of 1,250–1,350 K**, i.e. **300–1,500 cd/m²** of flame
  surface [E].
- Check against a campfire [E]: about 1 m² of flame at 500 cd/m² gives about 55 lx at 3 m, enough to
  read by.
- Total radiant emission: surface emissive power of large fires 50–100 kW/m² [R] corresponds to
  σT⁴ = 969–1,152 K effective (IR-weighted). Most of the power is infrared; the luminous efficacy of
  1,300 K radiation is 0.013 lm/W (§1.3).

**Size** [R: Heskestad]: mean flame height L = 0.235·Q^(2/5) − 1.02·D (Q in kW, D in m).
- A fully involved two-storey wooden house (Q ≈ 30–100 MW [E], D ≈ 8 m) has flames 10–20 m high above the
  fuel bed.
- Projected flame area is about 50–150 m² [E].

**Derived luminous intensity** [D/E]: 50–150 m² × 300–1,000 cd/m² ≈ **1.5×10⁴–1.5×10⁵ cd**. The central
value is 5×10⁴ cd.
- It gives 125 lx at 20 m, 20 lx at 50 m, 1.25 lx at 200 m and 0.2 lx at 500 m.
- A single burning house outshines a full Moon to about 500 m.
- **S/P = 0.45–0.5** (§6.2): rods see firelight poorly.
- Flicker: about 1–3 Hz puffing for pool-like fires of metre scale (Zukoski's f ≈ 1.5/√D) [R]. Burning
  timber collapses in steps.
- Burnt-out ruins (most of the January 1943 centre, per `docs/URBAN_INTRO_20260928.md:39`) hold **embers
  at 900–1,100 K**: 0.2–23 cd/m², a dull red glow visible only to a dark-adapted eye.

### 4.5 Burning vehicles [E]

**Fuel and burning rate:**
- A tank carries several hundred litres of gasoline (Tiger) or diesel (T-34), plus propellant, rubber,
  paint and oil.
- Pool-fire mass burning rates [R: Babrauskas, SFPE Handbook]:
  - gasoline about 0.055 kg m⁻² s⁻¹, ΔH_c 43.7 MJ/kg, so about 2.4 MW/m²;
  - diesel about 0.045 kg m⁻² s⁻¹, about 2 MW/m².

**Estimate:**
- A burning hull with 2–5 m² of exposed fuel surface: **5–15 MW**, flames 5–10 m.
- At 1,250–1,350 K effective and 10–30 m² projected area: **5×10³–3×10⁴ cd** (central 1.5×10⁴).
- Duration: tens of minutes to hours, with cook-off flashes.
- The current wreck light reads 1.4×10⁶ cd (§4.9).

### 4.6 Muzzle flash

**Measured** [M: B. J. Steward, *Characterization and Discrimination of Large Caliber Gun Blast and
Flash Signatures*, AFIT dissertation 2011, DTIC ADA552974]. 201 firings of a 152 mm howitzer; full
charge 8.99 kg propellant, 43.5 kg projectile, 638 m/s:
- muzzle flow expanded into three plumes of 2–3 m radius, side-on projected area 35–40 m²;
- secondary combustion (the muzzle flash proper): **about 100 ms, 1,200–1,600 K**, emissive area
  20–35 m², "dominated by selective radiation";
- two-region fit at peak: core 20.7 m² at 1,548 K; thin flame front 5.0 m² at 2,552 K;
- non-combusting (suppressed) plumes: about 20 ms, 850–1,050 K, continuum emissivity about 0.36;
- secondary combustion is 10–100× brighter than non-combusting plumes and 100–1,000× the intermediate
  flash;
- more than 99 % of the radiated energy is infrared (citing the *Handbook on Gun Flash*);
- the visible spectrum shows Na D (589 nm), Li (670.8 nm) and K lines on a soot continuum, i.e. yellow
  to orange.

**Scaled to the Tiger's 8.8 cm KwK 36** [D/E]:
- The project's sound report gives 2.5 kg of propellant (`docs/research/SOUND_LEVELS_20260928.md:47`).
- Plume volume scales with gas mass, so area scales as m^(2/3): (2.5/8.99)^(2/3) = 0.43, giving
  **8.5–15 m²**.
- Duration ∝ m^(1/3), giving **about 65 ms**.
- Core at 1,450–1,550 K with visible emissivity 0.2–0.5 [E]: 1,000–7,000 cd/m².
- So **I_v ≈ 2×10⁴–1×10⁵ cd for about 65 ms**. It would be less with a cool, flash-reduced diglycol
  charge; the charge composition was not retrieved.
- T-34 76.2 mm (1.08 kg): area × 0.24, about 5–8.5 m², **1–5×10⁴ cd, about 50 ms**.
- A muzzle brake splits the flash into side lobes, as on the 152 mm.
- **The flash does not light the landscape like a flare.** At 100 m it gives 2–10 lx for 0.07 s. At
  500 m, 0.08–0.4 lx, about moonlight.
- A combusting flash is not guaranteed; suppressed or cold plumes do not ignite.

### 4.7 Tracers [E]

- **Composition:** red tracers use strontium [S: AMCP 706-185 §6-3.3: "Most tracers are made to produce
  red light because red light is the most visible color under daylight conditions"].
- **Trace length:** US .30 M1 about 1,300 yd, .50 about 2,200 yd [M: AMCP 706-185 §1.2].
- **Small arms (7.62–7.92 mm):**
  - 0.3–0.5 g of composition over 2–3 s at 5,000–12,500 cd·s/g (Sr flare values of AMCP 706-185; spin
    lowers output [M]) gives ≤ 500–2,000 cd;
  - take **300–1,000 cd**, red, about 2 s.
- **88 mm Pzgr 39 base tracer** (designation "L'spur", [M: TM 9-1985-3 nomenclature]): 5–15 g over
  2–4 s, **10⁴–5×10⁴ cd** [E].
- **Visibility:** a tracer is a moving point source. At night its visibility follows Allard's law (§5.1).
  At 1 km a 500 cd tracer gives 5×10⁻⁴ lx at the eye, about 10⁵ times the dark-adapted point threshold
  (§6.7). It is conspicuous but lights nothing.

### 4.8 Doctrine for placing sources [A]

*Night Combat* (DA Pam 20-236) lists, as used on the Eastern Front:
- "Very lights, parachute flares, searchlights, machine gun tracer fire, mortar salvos at prearranged
  orienting points";
- "illumination of enemy terrain by artillery fire on inflammable targets";
- "The no man's land should be lit by flares and searchlights placed in flanking positions."

The climate pamphlet DA Pam 20-291 records that in the winter of 1942–43 "along the lower Don and
Donets... the first snow fell in mid-December", and gives a 12-day December blizzard near Voronezh.

### 4.9 Calibration of the existing game lights [C/D]

Physical combine: surface shading computes `source = ci.rgb * ci.w * spherical_emitter_attenuation(d², r)`
and multiplies it by the BRDF (`shaders/combine.wgsl:836-839`); the volume in-scatter term does the same
(`:743-757`). Both are in the same RU as the Sun. So a `PointLight {color, radiant_intensity}`
is a spectral radiant intensity triplet in W sr⁻¹ nm⁻¹.

| Light | Code | Triplet (R, G, B) | Reads as (§1.2) | Physical target | Ratio |
|---|---|---|---|---|---|
| Own muzzle flash | `420*f`, `(1,.74,.40)` `src/game.rs:16945` | (420, 311, 168) | 2.3×10⁷ cd | 2×10⁴–1×10⁵ cd, 65 ms | 230–1,100× |
| Enemy muzzle flash | `220`, `(1,.70,.34)` `:17044` | (220, 154, 75) | 1.1×10⁷ cd | 1–5×10⁴ cd | 230–1,100× |
| Burning wreck | `48*flick`, `(1,.30,.08)` `:17034` | (48, 14, 3.8) | 1.4×10⁶ cd | 5×10³–3×10⁴ cd | 50–280× |
| Editor preset "fire" | 95, 1,850 K, tint (1,.72,.46) `src/environment_authoring.rs:91` | (95, 15, 0) | 2.0×10⁶ cd | 10²–10⁴ cd (campfire to room fire) | 10²–10⁴× |
| Tableau stair-head pressure lamp | 5, 2,800 K (catalog) | (5, 2.0, 0.58) | 1.7×10⁵ cd | 300–500 cd (Petromax-class) [E] | ~400× |
| Tableau dugout stove | 2.5, 1,850 K | (2.5, 0.55, 0) | 6.0×10⁴ cd | 2–70 cd (0.1 m² door at 1,100–1,300 K) [D] | 10³–10⁴× |

- Colour uses the Tanner-Helland sRGB fit of the temperature (`kelvin_srgb`,
  `src/environment_authoring.rs:2641`) rather than a Planck spectrum. At 1,850 K it has a zero blue lane.
- The stylized backend reads the same values through a capped irradiance, `min(w/d², 12)`
  (`shaders/stylized_mesh.wgsl:545`). They were tuned to look right at the physical default exposure of
  320 in daylight. At a real night exposure they would burn out the frame.

### 4.10 Emitters in renderer units [D]

Triplets are W sr⁻¹ nm⁻¹ at 680 / 550 / 440 nm. Each carries the true photometric integral of a Planck
spectrum at the stated temperature.

| Source | I_v | T | Triplet | S/P |
|---|---|---|---|---|
| LC 50 F (4 candles) | 6×10⁵ cd | 3,000 K | (14.1, 7.68, 2.65) | 1.48 |
| FB 50 | 2.16×10⁵ cd | 3,000 K | (5.07, 2.76, 0.953) | 1.48 |
| US 60 mm illuminating | 1.45×10⁵ cd | 3,000 K | (3.40, 1.85, 0.640) | 1.48 |
| White star 26 mm [E] | 2×10⁴ cd | 2,800 K | (0.518, 0.251, 0.074) | 1.39 |
| 88 mm flash, low | 2×10⁴ cd | 1,500 K | (1.58, 0.163, 0.0064) | 0.61 |
| 88 mm flash, high | 1×10⁵ cd | 1,500 K | (7.92, 0.815, 0.032) | 0.61 |
| Burning house | 5×10⁴ cd | 1,300 K | (5.40, 0.333, 0.0066) | 0.47 |
| Burning tank | 1.5×10⁴ cd | 1,300 K | (1.62, 0.100, 0.0020) | 0.47 |
| Pressure lamp | 400 cd | 2,800 K | (0.0104, 0.0050, 0.0015) | 1.39 |
| Stove door | 30 cd | 1,300 K | (0.0032, 2.0×10⁻⁴, 4×10⁻⁶) | 0.47 |

- The flare's colour temperature of 3,000 K is an assumption: Mg/Al with a Ba nitrate flame has band
  emission. The photometric integral is what is anchored.
- Real flames and flashes should be **area or volume emitters**, radiance = ε·B_λ(T) over the modelled
  flame surface. Compact points are a debug fallback.
- The triplet is the 3-band reduction of that emitter. For fire temperatures the band values should be
  scaled so that the §1.2 weights reproduce the true integral: divide by the "reads high" factors 1.4–1.9
  of §1.2.

---

## 5. Falling snow, smoke and haze as media (deliverable 4)

### 5.1 Visibility definitions

- **Koschmieder:** daytime visual range of a black object against the horizon sky, V = ln(1/ε)/σ_ext.
  With contrast threshold ε = 0.02, V = 3.912/σ [S].
- **Meteorological optical range (WMO):** 5 % transmittance, MOR = 2.996/σ [S: WMO CIMO Guide].
- **Allard's law for lights at night:** a light of intensity I is seen at distance d when
  I·e^(−σd)/d² ≥ E_t [S]. E_t, the observer's point-source threshold, is set by adaptation, not by the
  medium (§6.7).
- **The game's Act I whiteout,** σ_t = 0.0068 m⁻¹ (`docs/PHYSICAL_ENVIRONMENT_TRANSPORT.md` §Act-I):
  MOR 441 m, Koschmieder 575 m [D]. That is a heavy snowfall or blowing snow.

### 5.2 Falling snow

**Extinction against snowfall rate** [D], for geometric-optics flakes:
```
β_ext = Q_ext · (projected area per mass) · IWC
      = Q_ext · 1.5/(ρ_s·D) · S/v
```
- S: water-equivalent mass flux (1 mm/h = 2.78×10⁻⁴ kg m⁻² s⁻¹);
- v: fall speed;
- ρ_s: flake bulk density;
- D: flake diameter.

| Snow type (all at 1 mm/h w.e.) | v | ρ_s | D | β | MOR |
|---|---|---|---|---|---|
| Dry, fluffy aggregates | 1 m/s | 50 kg/m³ | 5 mm | 3.3×10⁻³ m⁻¹ | 900 m |
| Typical aggregates | 1 m/s | 100 kg/m³ | 3 mm | 2.8×10⁻³ m⁻¹ | 1,080 m |
| Dense or rimed | 1.5 m/s | 200 kg/m³ | 2 mm | 1.4×10⁻³ m⁻¹ | 2,160 m |

- β scales linearly with S.
- **The game's 0.0068 m⁻¹ equals about 2.4 mm/h w.e.** of typical aggregates.
- Observers' classes [R: US FMH-1 §8, snow intensity by visibility]: light > 800 m; moderate 400–800 m;
  heavy ≤ 400 m. They are consistent with the derivation.
- Rasmussen et al. (1999, *J Appl Meteor* 38:1542), the standard visibility–snowfall theory, could not be
  retrieved. It also discusses why Q_ext is effectively between 1 and 2: the diffraction lobe is caught
  by the receiver.

**Diffraction lobe** [D]:
- For mm-sized flakes Q_ext → 2, but half of that is diffraction into about λ/D ≈ 2×10⁻⁴ rad (0.01°).
- For imaging at this game's resolution (0.1° per pixel or more) the lobe is indistinguishable from
  unscattered light.
- The honest choice is delta-scaling: σ′ = σ(1−f) and g′ = (g−f)/(1−f) with f ≈ 0.5 for the diffraction
  fraction. The effective extinction for image contrast is then about half the geometric Q = 2 value.
- The delta-Eddington closure the renderer already uses (f = g²) implements the same idea.

**Single-scattering albedo** [D from M: Warren & Brandt 2008]:
- Per interaction, for an ice path of about 1 mm: 1−ω ≈ γ·ℓ = 1.8×10⁻⁶ (440 nm), 5.2×10⁻⁵ (550 nm),
  3.9×10⁻⁴ (680 nm).
- So **ω ≈ 0.999998 / 0.99995 / 0.9996**: conservative, very slightly red-absorbing.
- The game's bulk 0.995 ("neutral") absorbs about 100× too much in the blue-green. That is harmless for
  thin optical depths. Over the snowpack's thousands of scatterings it would not be.

**Phase function:**
- Hexagonal ice crystals and aggregates have asymmetry g ≈ 0.75–0.89, including diffraction [R: Macke
  et al. 1996; Kokhanovsky].
- The game's snow endpoint of 0.76 is inside that range.
- Plate crystals falling in orientation (diamond dust, ice fog at −15 to −30 °C) add light pillars above
  and below lamps, flares and fires, and 22° halos. An HG phase cannot produce these; a tabulated
  oriented-plate phase would be needed [S: standard atmospheric optics; E for its frequency at
  Stalingrad].

**Resolved flakes near a light:**
- Within a few metres of a lamp or flare, individual flakes are resolved sparkles. The High preset
  already carries resolved grains (`docs/PHYSICAL_TRUTH_AUDIT.md:56`).
- A muzzle flash lights them for 65 ms.

### 5.3 Smoke

**Fire-engineering defaults** [S: NIST FDS source, fetched]:
- soot mass extinction coefficient **8,700 m²/kg** (`read.f90`: `SS%MASS_EXTINCTION_COEFFICIENT =
  8700._EB`);
- visibility S = C/K with **C = 3** (`cons.f90`: `VISIBILITY_FACTOR=3._EB`), the value for
  light-reflecting objects. C = 8 applies to light-emitting signs (Jin) [R].

**Wood and biomass smoke** [M: Reid et al. 2005, *ACP* 5:827]:
- fresh-smoke mass scattering 2.8–4.2 m²/g (mean about 3.6);
- flaming-phase absorption 1–1.4 m²/g, up to 3 m²/g for black plumes from very intense fires;
- mean ω₀ about **0.75** for flaming-dominated smoke;
- higher for smouldering smoke (larger particles, less black carbon);
- HG g = 0.44–0.63, with about **0.63** for aged smoke.

**Black soot from fuel, rubber and burning tanks:** ω₀ ≈ 0.2–0.3 [R: Bond & Bergstrom 2006].
- Take extinction 8.7 m²/g, ω₀ 0.25, g 0.6.

**Recommended pairs:**
- burning buildings and wood: ω₀ 0.75 (flaming) to 0.9 (smouldering), g 0.6;
- fuel fires and vehicles: ω₀ 0.25, g 0.6.

**"Dark soot shades" colouring,** mentioned in `docs/PHYSICAL_TRUTH_AUDIT.md:81`: smoke colour must
come from ω and g lit by the scene, not from an authored shade.

### 5.4 Haze and ice fog

- **The game's aerosol:** Bruneton's default, ω = 3.996/4.44 = 0.90, g = 0.80, scale height 1.2 km [C
  `src/physical_environment.rs:79-86`]. That is reasonable for continental winter haze.
- **Mineral dust:** "most atmospheric aerosol apparently has ω ≈ 0.96 or even higher at 550 nm" [M:
  quoted in Dang et al. 2015].
- **Ice fog and diamond dust:** a clear, calm, very cold night (−25 °C, as in January 1943) forms ice
  fog under an inversion. Its particles are 10–50 µm crystals, with ω ≈ 1 and g ≈ 0.8. Its visibility
  can drop to a few hundred metres, and it produces pillars (§5.2) [E].
- **Remove the night fog multiplier.** The day-phase ×1.10 on `fog_density`
  (`src/renderer.rs:5507, 5523`) should become weather state (radiation fog or ice fog), not a
  day-phase multiplier.

### 5.5 Light in the air (owner item 1) [D]

**Model.** Single-scattered radiance along a view ray passing a point source of intensity I at closest
distance b, in a homogeneous medium (σ_s, σ_t, HG g), camera 200 m before the closest approach. The
radiance is the numerical integral of σ_s·p(θ)·I·e^(−σ_t(r+s))/r². For isotropic scattering without
attenuation it reduces to σ_s·I/(4b).

| Medium | Source | b = 2 m | b = 10 m | b = 50 m |
|---|---|---|---|---|
| Light snow (σ 1×10⁻³, g 0.8) | Burning house 5×10⁴ cd | 15.9 cd/m² | 2.6 | 0.18 |
| | Pressure lamp 400 cd | 0.13 | 0.021 | 0.0014 |
| | LC 50 flare 6×10⁵ cd | 190 | 31 | 2.1 |
| Heavy snow (6.8×10⁻³, g 0.76, game) | Burning house | 28.6 | 4.8 | 0.37 |
| | Lamp | 0.23 | 0.039 | 0.0029 |
| | Flare | 343 | 58 | 4.4 |
| Wood smoke (9×10⁻³, g 0.6) | Burning house | 15.4 | 2.8 | 0.30 |
| Haze (2×10⁻⁴, g 0.7) | Burning house | 2.6 | 0.46 | 0.047 |

Reading:
- A fire's glow in falling snow (0.2–5 cd/m² tens of metres from the flames) is 5–100× brighter than the
  moonlit snowfield around it (0.05 cd/m²).
- A pressure lamp's glow at 10 m is comparable to moonlit snow.
- Real shadows follow from the same integral with a visibility term. Ruins crossing a fire's or flare's
  light cast dark shafts in the lit snowfall. That is transport, not a painted god-ray, and is permitted.

---

## 6. The human eye (deliverable 5)

### 6.1 Operating ranges

- **Mesopic range:** CIE 191:2010 defines the mesopic range for photometry as **0.005–5 cd/m²**, and the
  mesopic adaptation coefficient as m = 0.767 + 0.3334·log₁₀(L_mes), iterated and clipped to [0, 1], with
  L_mes = [m·L_p + (1−m)·L_s·V′(λ₀)]/[m + (1−m)·V′(λ₀)] and V′(λ₀) = 683/1699 [R].
- **Duplicity** [M: Kalloniatis & Luu, *Webvision*, "Light and dark adaptation"]: cones mediate vision
  above about 0.03 cd/m²; rods below.
- **Absolute threshold** after about 40 min in the dark: about **10⁻⁵ cd/m²** (extended test field),
  from the same source.
- **Photopic:** above about 5–10 cd/m².
- **In battle terms** (§2.7):
  - daylight and fire-lit ground within about 50 m of a fire are photopic;
  - a full Moon, flare light and civil dusk are mesopic;
  - a quarter Moon, starlight and a moonless overcast are scotopic.
  - Near the scotopic floor a moonless overcast (6×10⁻⁵ cd/m²) is less than a decade above absolute
    threshold: shapes only.

### 6.2 Spectral sensitivity and the Purkinje shift

**Peaks:** from the CVRL 1-nm tables, CIE V(λ) peaks at **555 nm** and V′(λ) at **506–507 nm**. The
integrals are ∫V = 106.86 nm and ∫V′ = 97.07 nm [S/D].

**S/P ratio** (scotopic ÷ photopic luminance) of the battle's light sources [D]:

| Source | S/P |
|---|---|
| Sun / moonlight | 2.3–2.4 |
| Natural night sky | about 2.1 |
| Flare (3,000 K) | 1.48 |
| Pressure lamp (2,800 K) | 1.39 |
| Candle or stove (1,850 K) | 0.85 |
| Muzzle-flash core (1,500 K) | 0.61 |
| Burning house (1,300 K) | 0.47 |
| Embers (1,100 K) | 0.33 |

Consequences, physiologically grounded:
- Rods, the night system, respond to firelight 3–5× less than to moonlight of equal photopic luminance.
- In mixed scenes the firelit patch stays **cone-mediated and coloured**, while the moonlit surround is
  **rod-mediated and achromatic**.
- Reds go dark before blues as light falls: the Purkinje shift.
- Rod vision carries no hue. A blue tint on moonlit scenes is a perceptual report, not rod physiology.
  If used at all, it must be declared a perceptual model, not physics.

### 6.3 The pupil

**Size** [R: Watson & Yellott 2012, *J Vis* 12(10):12, doi:10.1167/12.10.12, "A unified formula for
light-adapted pupil size". The abstract was read via PubMed; the constants below are from the paper as
recalled and must be verified.]
```
D_SD(L, a) = 7.75 − 5.75·[x/(x+2)],  x = (L·a·M/846)^0.41   (Stanley & Davies; a field area deg², M = 1 binocular, 0.1 monocular)
D_U = D_SD + (y − 28.58)·(0.021323 − 0.0095623·D_SD)         (age y in years, 20–83)
```

| Condition (age 25) | 10⁻⁴ cd/m² | 10⁻³ | 10⁻² | 0.05 | 1 | 100 | 10⁴ |
|---|---|---|---|---|---|---|---|
| Binocular, 3,000 deg² field | 7.8 mm | — | 7.3 | — | 5.2 | 2.9 | 2.15 |
| Monocular through the sight, 25° field (491 deg²) | — | 7.9 | — | 7.7 | 7.1 | 4.9 | — |

**Dynamics** [M: *A Mathematical Model of Flashblindness*, 1965, DTIC AD0627332, citing Miller's
laboratory]:
- pupil response latency "possibly as short as 120–140 msec";
- "some… subjects give a maximum constriction in 1 sec or so";
- dark-adapted pupil area about **50 mm²** (8 mm).
- Dilation is slower. Take a fast phase of about 2 s and a slow tail of 10–20 s [E]. The Ellis 1981 data
  (*Br J Ophthalmol* 65:754, PMC1039657) are page scans only and could not be read.

**Age** [M: flash-blindness review, DTIC AD0840277 (1968)]: "for every increase of 13 years in age,
intensity of illumination must be doubled to be just seen by the fully dark adapted eye".

### 6.4 Light adaptation (fast)

- **Neural gain:** the rapid adaptation mechanism acts "within 20–200 ms" in the photoreceptors. The rod
  spatial adaptation pool is about **10 arcmin** [M: Mantiuk et al. 2011 (HDR-VDP-2), citing He &
  MacLeod 1998].
- **Threshold against adaptation** [R: Ferwerda, Pattanaik, Shirley & Greenberg 1996, SIGGRAPH, "A
  model of visual adaptation for realistic image synthesis"]. Coefficients recalled; I checked
  continuity at every branch point, within 0.012 log units. L_a in cd/m² for cones and scotopic cd/m² for
  rods:
```
cones: log ΔL = −0.72                          (log L_a ≤ −2.6)
                (0.249·log L_a + 0.65)^2.7 − 0.72 (−2.6 < log L_a < 1.9)
                log L_a − 1.255                  (log L_a ≥ 1.9)
rods:  log ΔL = −2.86                          (log L_a ≤ −3.94)
                (0.405·log L_a + 1.6)^2.18 − 2.86 (−3.94 < log L_a < −1.44)
                log L_a − 0.395                  (log L_a ≥ −1.44)
```
- At full-moon snow (0.049 cd/m², 0.11 scotopic cd/m²):
  - the cone Weber fraction ΔL/L is 4.4;
  - the rod Weber fraction is 0.40.
  - Rods carry the scene.
- Measured rod TVI slopes [M: Webvision]: de Vries–Rose about 0.5–0.6; Weber 0.8 or less; rods saturate
  at high backgrounds.
- A rod-saturating background is about 1,700 scotopic td [M: Mahroo & Lamb 2004].

### 6.5 Dark adaptation (slow)

**Curve shape** [M: Webvision, reproducing Hecht, Haig & Chase 1937]:
- an initial fast drop;
- a cone plateau;
- the **rod-cone break after 5 to 8 minutes**;
- the curve "asymptotes to a minimum (absolute threshold) at about 10⁻⁵ cd/m² after about forty
  minutes".
- The break is absent for deep-red test lights, which rods and cones see equally.

**Cone pigment** [M: Mahroo & Lamb 2004, *J Physiol* 554:417, PMC1664777]. Regeneration is
**rate-limited**:
```
dB/dt = −v·B/(B + K_m) + (1−B)·I/σ⁻¹
```
- K_m = 0.2;
- v = 0.47–0.50 min⁻¹ ("the initial rate of recovery following a total bleach was ca 50 % of the total
  pigment per minute");
- σ⁻¹ = 710 cd m⁻² min with a dilated pupil;
- half-bleach intensity 570–610 cd/m² (dilated);
- recovery "within about 4 min";
- best exponential fit τ = 1.02–1.04 min.
- With an 8 mm pupil (50 mm²), σ⁻¹ ≈ 3.6×10⁴ td·min = **2.1×10⁶ td·s** [D].

**Rod pigment:** Lamb & Pugh 2004 (*Prog Retin Eye Res* 23:307; abstract read) show that rod recovery is
rate-limited by the delivery of 11-cis retinal, over tens of minutes.
- Classical rhodopsin regeneration τ ≈ 400 s [R: Rushton].
- A rate-limited model with K_m = 0.2 then needs v_r = K_m/τ ≈ **0.03 min⁻¹** [D]. A full bleach
  then takes about 30 min to reach 10 % residual, matching the 30–40 min curve.
- Rod photosensitivity: σ_r⁻¹ ≈ 10⁷ scotopic td·s [R].

**Threshold from bleaching** [M: Webvision, Rushton]:
- "bleaching 50 % of rhodopsin in rods raises threshold by 10 log units while the bleaching 50 % of cone
  photopigment raises threshold by about one and a half log units";
- so α_r ≈ 20 and α_c ≈ 3 log units per unit fraction bleached, with log(ΔT/ΔT₀) = α·B.

### 6.6 Flash blindness and recovery

**Measured** [M: *A Review of Research on Flash Blindness*, 1968, DTIC AD0840277, summarising Miller's
studies]:
- "The only portion of flash radiation that influenced the recovery times for foveal performance was in
  the visible region. Infrared had no effect."
- A small effect of flash-field size (2.5–10°).
- "An approximately linear relationship… between the logarithm of retinal illuminance times the
  duration of the flashes and logarithm of the recovery times for the recognition of a 20/60 acuity
  target at 0.06 millilambert, over the range of **20 seconds to 130 seconds** recovery time,
  corresponding to a range of **9×10⁵ to 3×10⁷ troland-seconds** flashes."
- "No significant cumulative effect on recovery times with successive flashes after the second flash"
  at 3–4 min intervals.
- For the largest flash, log recovery fell from 1.8 (63 s) to 0.6 (4 s) as target luminance rose from
  10^−0.8 to 10^2.2 mL, a slope of about −0.4.

**Minimum recovery time:** about 0.2 s, the visual reaction time. Recovery levels off above
7.4 log td·s [M: AD0627332, citing Brown].

**Fit** [D], with the review's two slopes:
```
log₁₀ t_rec [s] = log₁₀ 20 + 0.53·log₁₀(E_flash / 9×10⁵ td·s) − 0.4·log₁₀(L_target / 0.19 cd/m²),   t_rec ≥ 0.2 s
```
It is valid inside 9×10⁵–3×10⁷ td·s. Below that range the fit is an extrapolation [E].

**The gunner and his own 88** [D/E]:
- Flash luminance 1,000–7,000 cd/m² (§4.6) for 65–100 ms.
- Sight transmission 0.6–0.85 [E].
- Dark-adapted pupil about 7.5 mm (44 mm²).
- E_flash ≈ 6×10³–3×10⁴ td·s.
- Recovery to see:
  - a 0.19 cd/m² target: **1–3 s**;
  - moonlit snow (0.05 cd/m²): **2–5 s**;
  - a starlit or overcast-dark scene (≤ 10⁻⁴ cd/m²): **0.4–1 min**.
- Cone bleach from one flash: 6×10³/2.1×10⁶ ≈ 0.3 %, i.e. 0.01 log units by the Rushton relation. The
  loss is neural and afterimage-driven, not pigment exhaustion.
- A flare overhead for 5 min (snow at 1.9 cd/m², pupil about 4.8 mm, about 35 td, about 50 scotopic td)
  bleaches only about 0.2 % of the rods. After burnout, recovery is neural: seconds to about a minute
  [D].
- Looking directly at the flare or a fire (a point of 10⁶ cd/m² or more) leaves a local afterimage on
  that retinal patch.

### 6.7 Point sources and acuity

- **Point-source threshold:** naked-eye limiting magnitude about 6.0 under a dark sky is
  2.54×10⁻⁶ × 10^−2.4 ≈ **1.0×10⁻⁸ lx** [D]. Background light, including any flare or fire in view, raises
  it by orders of magnitude.
- **Point-source angular size** [M: AMCP 706-185 Table 4-3]: a source is a "point" if smaller than
  0.708′ at 1,000 fL, and up to 15′ at 10⁻⁵ fL.
- **Flicker:** flashes shorter than 0.2 s need a higher intensity to be seen [M: AMCP 706-185 §4].
- **Acuity:** the scotopic loss of acuity and colour (Shlaer 1937, PMC2141937, scans only) is
  physiological. It can be modelled by a low-pass tied to the rod pool (about 10′) at scotopic levels,
  declared as an observer effect [E].

### 6.8 Veiling glare (owner decision required)

- Intraocular scatter adds a veil L_veil = 10·Σ E_i/θ_i² (θ in degrees, 1–30°; E_i the illuminance at the
  eye from each glare source) [R: CIE 146:2002, Stiles–Holladay form].
- Example [D]: a burning house at 50 m (20 lx at the eye), 5° off the line of sight, gives about
  **8 cd/m²** of veil. That is 160× moonlit snow. Near fires, dark detail vanishes.
- This is ocular physiology, not atmosphere. It is still a "veil" in CLAUDE.md's language. It needs an
  explicit owner decision, a debug gate and a ledger row before use. Without it, a night scene with
  fires is rendered unrealistically clear.

### 6.9 A real-time observer model

It is a reproduction transform, not physics, like the audio chain's. It runs per view, on the HDR buffer
after transport and before bloom and crush. It never writes into transport buffers. Its constants are
identical in both presets; only its resolution may differ.

1. **Meter (photometric).**
   - HDR is stored pre-exposed: H = RU × P_prev.
   - Compute Y_p = K_p·H/P_prev and Y_s = K_s·H/P_prev (§1.2), in cd/m².
   - Downsample to about 64×36 log-luminance. Blend a foveal 2° disc, a 20° Gaussian and the whole
     field, with weights 0.5/0.3/0.2 [E], into L_a,p and L_a,s.
   - Clamp to 10⁻⁷–10⁷ cd/m². The present floor is 36 cd/m².
2. **Pupil.**
   - Target D* from §6.3, with a = field area (monocular with M = 0.1 through the sight; a = 491 deg² for a
     25° field).
   - First-order response: 0.2 s latency, τ 0.3 s constricting, 2 s dilating, plus a 15 s tail [M/E].
   - Area A = πD²/4.
3. **Retinal illuminance.** I_p = L_a,p·A·τ_optic and I_s = L_a,s·A·τ_optic, with τ_optic = 1 for the
   naked eye and 0.6–0.85 [E] through the TZF sight or a periscope.
4. **Pigments.**
   - Integrate B_c and B_r with §6.5: cones K_m 0.2, v 0.5 min⁻¹, σ⁻¹ 2.1×10⁶ td·s; rods K_m 0.2,
     v 0.03 min⁻¹, σ⁻¹ 10⁷ scotopic td·s.
   - Explicit Euler is stable at game dt.
5. **Neural state.**
   - A log-domain first-order filter per system: cones τ_up 0.1 s / τ_down 1 s; rods τ_up 0.2 s /
     τ_down 3 s [E, bounded by the 20–200 ms of §6.4].
   - Floor it with the bleaching threshold elevation ΔT = ΔT₀·10^(α·B), α_r = 20, α_c = 3. Take the
     adaptation state as the TVI-inverse of that threshold (the "equivalent background").
   - ΔT₀ is the absolute threshold: 10^−0.72 cd/m² for cones, 10^−2.86 scotopic cd/m² for rods, from the
     TVI floors.
6. **Visibility mapping** (display reproduction).
   - Pick a display adaptation, e.g. half of display white: L_da = 100 cd/m² for a 200 cd/m² display.
   - Scale factors: s_c = TVI_c(L_da)/TVI_c(A_c) and s_r = TVI_c(L_da)/TVI_r(A_r).
   - Display luminance = m·s_c·Y_p (chromatic) + (1−m)·s_r·Y_s (achromatic, spectrally neutral), with m
     from CIE 191 at L_a. Divide by display white, then ACES and sRGB as today.
   - Examples [D], as a fraction of display white for the adapting surface:
     - full-moon snow ≈ 7 % (rods);
     - quarter moon ≈ 5 %;
     - moonless clear ≈ 0.6 %;
     - moonless overcast ≈ 0.3 %.
   - Photopic scenes map to about 50 %.
   - A night stays dark but legible, as the visual system's thresholds dictate. A fixed-key auto
     exposure would instead show a moonless night like day.
7. **Flash state.**
   - Per frame, accumulate retinal energy over bright pixels into a coarse retinal map, e.g. 32×18
     centred on fixation.
   - Drive a local equivalent background that decays with the §6.6 fit. That yields afterimages and
     recovery times of 1–60 s.
   - A global-only first version is acceptable.
8. **Pre-exposure for the next frame.** P = 0.18/(L_a,p/72,983) (RU), clamped so that the brightest
   expected radiance × P stays below 6×10⁴ (binary16).

---

## 7. Code reading (deliverable 6)

### 7.1 Two architectures [C]

- **Backend selection:** `KESSEL_RENDERER` picks it. The default is **Physical**
  (`src/graphics.rs:25-31`), used by headless screenshots. The playtest launcher exports
  `KESSEL_RENDERER=shader` (`tools/launch_shader_playtest.sh:35`, via
  `tools/launch_winter_session.sh:36`).
- **Physical backend:**
  - Deferred G-buffer and `combine.wgsl` when hardware ray query is present (`deferred: rt_on`,
    `src/renderer.rs:5371-5376`).
  - Otherwise the forward `mesh.wgsl` (`:3438`), which has its own sun (1.74) and "moon" (0.21) constants
    (`shaders/mesh.wgsl:354-360`). That is a second physical law, selected by hardware; see §7.8.
  - Sky, sun and environment come from `physical_environment.wgsl` (Bruneton-type spherical atmosphere,
    delta-Eddington cloud; `docs/PHYSICAL_ENVIRONMENT_TRANSPORT.md`).
- **Shader backend:** `stylized_mesh.wgsl` and `stylized_sky.wgsl`, with the empirical daylight laws of
  `solar_daylight.wgsl`.

### 7.2 Exposure plumbing [C]

- Physical calibration constant: `DEFAULT_CAMERA_EXPOSURE = 320.0`. The doc comment reads: "Actual
  recorded Playtest HDR cameras metered ~257 outdoors and ~350 inside" (`src/renderer.rs:750-754`).
  - In photometric terms, mid-grey 0.18 corresponds to Y = 0.18/320 = 5.6×10⁻⁴ RU ≈ **41 cd/m²**, i.e.
    **EV100 ≈ 8.4** [D].
- Crush exposure: `exposure_override.unwrap_or(if shader_backend {1.25} else {320})`
  (`src/renderer.rs:7002`). The debug capture repeats it (`:8818`).
- The authored lighting writes the override with no backend check:
  `self.exposure_override = Some(tuning.camera_exposure)` (`src/renderer.rs:5483`), called at build
  (`src/game.rs:11551`) and every frame (`src/game.rs:14480`).
- Authoring range: `LightingTuning` default 0.55 (`src/environment_authoring.rs:294`); `from_live` clamps
  to 0.02–8 (`:321`); `validate` enforces 0.02–8 (`:353`).
- The environment override `KESSEL_CAMERA_EXPOSURE` accepts 0.01–360 (`src/renderer.rs:399-420`).
- Workshops: courtyard, urban intro and the staged encounter all set 1.25; the ruined-house courtyard
  sets 1.5 (manifest `environment_lighting`, read with `python3 json`).
- The eye pass runs only if `autoexp && exposure_override.is_none()` (`src/renderer.rs:7630`). Gameplay
  construction sets `autoexp: false`, and the test enforces it: "gameplay must never start eye
  adaptation" (`:11047-11069`).
- `exposure.wgsl` meter:
  - Rec.709 weights on RU (`:45`);
  - `METER_MIN_LUMINANCE 0.0005` RU (≈36 cd/m²) and `MAX 64` (`:26-32`);
  - target clamp 0.16–360 (`:97-101`);
  - time constants 0.22 s and 1.80 s (`:121`);
  - comment: "Full rod adaptation is intentionally not simulated" (`:115-116`).
- The stylized raster optics use their own `exposure_override.unwrap_or(1.25)`
  (`src/raster_optics.rs:397, 549`).

### 7.3 Sky and sun [C]

- **Physical:**
  - Solar TOA irradiance (1.474, 1.8504, 1.91198) RU (`src/physical_environment.rs:87`), attenuated by a
    real atmosphere and by the liquid-water cloud.
  - Act I cloud: LWC 0.25 g m⁻³, r_eff 10 µm, 900–2,200 m, coverage `overcast` (`src/renderer.rs:654-667`,
    `:6935`). β = 3·Q·LWC/(4ρr) = 0.0375 m⁻¹, so τ ≈ 49 at full coverage [D]. That is a thick
    nimbostratus.
  - The sun direction is the authored or day-phase vector; there is no ephemeris.
- **Stylized:**
  - `stylized_environment` and `hemisphere_parts` (`src/renderer.rs:9871-9892`,
    `shaders/stylized_mesh.wgsl:380-392, 524-531`).
  - Unitless sky, ground and sun palettes, multiplied by `diffuse_daylight`, the CIE-overcast Krochmann
    fit E ≈ 0.3 + 21·sin h klx, normalised to 31° (`shaders/solar_daylight.wgsl:8-12`).
  - Also `beam_daylight` (Kasten–Young air mass, Meinel channel transmittance) and `cloud_beam` (authored
    transmittance raised to an air-mass ratio) (`:14-34`).
  - Horizon and fog radiance: `horizon_sky_radiance` (`:39-44`).
- **At the urban camera** (overcast 0.96, storm 0.66, twilight 0.15, sun 7.5°) [D]:
  - stylized sky ≈ (0.021, 0.026, 0.033);
  - fog/horizon ≈ 0.066 (G);
  - direct sun ≈ (0.017, 0.012, 0.007).
  - The palette's 7.5° is **1.9 stops** below its 31° reference (Krochmann ratio 0.274). USNO's ratio is
    0.137, **2.9 stops**. The law itself is plausible for "average overcast".

### 7.4 The night floor [C/D]

- `DayPhase::night_factor`: Dawn 0.08, Day 0, Dusk 0.24, **Night 0.80**, with the comment "Preserve a
  little of the storm-day key so moonlit terrain stays navigable" (`src/renderer.rs:789-799`).
- `light_dir`: Night `(-0.24, 0.58, -0.78)` (`:809-816`).
- Physical path: `solar_irradiance *= 1.0 - night` (`:6936`). **The "moon" is the Sun at 20 %, from
  35.4°**: 16–22 stops too bright (§0.4).
- `game.rs:14637-14641` insists the physical path keep "a bounded solar/moon key at night (0.80)"
  because "writing 1.0 … made deferred Nacht frames black". That black frame is the fixed exposure 320
  meeting a physical night. §2.7 shows why no fixed exposure can display both.
- Stylized: `night_mix` blends to a fixed palette, sky 0.62×(0.055, 0.072, 0.105) and sun
  (0.14, 0.19, 0.29) (`src/renderer.rs:9878-9888`, `shaders/stylized_mesh.wgsl:383-392, 528-531`).
  - Its night sky (0.0446 in G) is **0.76 stops brighter than its own 7.5° overcast dusk sky** (0.0263)
    [D].
  - Night is only 1.4 stops (sky) to 2.9 stops (sun) below its 31° day.
- `mesh.wgsl` (the forward fallback): `moon_sun = 0.21` against `day_sun = 1.74`, i.e. night = 12 % of
  day (`shaders/mesh.wgsl:358-360`). That is about 15.6 stops too bright against real moonlight.
- Fog density ×1.10 at night (`src/renderer.rs:5507, 5523`).

### 7.5 Tone mapping and display [C]

- `crush.wgsl`: `c = hdr × exposure`; the ACES fit `(x(2.51x+0.03))/(x(2.43x+0.59)+0.14)`; clamp 0–1
  (`:33-36, 89-90`).
- Optional ordered dither and posterisation in display codes (`:95-126`); native 8-bit dither (`:127-137`).
- No black-point clip or contrast curve, by design (`:91-94`).
- The same ACES serves both backends. Only the exposure scalar and the radiance scale differ.
- In the toe, ACES(x) ≈ 0.214x·(1+79x), so dim scenes stay nearly linear. With x = 0.003 the output is
  0.0008 [D].

### 7.6 The six-stop disagreement, quantified [M/C/D]

- **Recorded:** mean final linear luminance 0.0008 (physical) against 0.051 (shader) from the same
  camera, both at exposure 1.25 (`docs/URBAN_INTRO_20260928.md:295-298`) [M in-project].
- **Physical frame:**
  - inverting ACES, mean x ≈ 0.0030 (an upper bound: ACES is convex in the toe);
  - mean scene Y ≈ 0.0030/1.25 = 0.0024 RU;
  - **≈ 150–175 cd/m²** [D].
- **Physical expectation** (§2.1, §2.7): at 7.5° under overcast (÷3 to ÷10), snow is 210–700 cd/m² and the
  overcast sky 233–778 cd/m². Dark ruins are 20–150 cd/m². A frame mean of about 150–500 cd/m² follows.
  The physical transport lands at the low end, consistent with a τ ≈ 49 storm deck plus blowing snow.
- **Shader frame:** mean x ≈ 0.055, i.e. unitless HDR ≈ 0.044. Pre-tonemap the stylized frame sits
  log2(0.055/0.0030) = 4.2 stops above the physical one. After ACES the gap reads as 6.0 stops.
- **Attribution:**
  - The physical path is **8.0 stops** under its own calibration (320 → 1.25);
  - it is **5.9 stops** under an 18 %-keyed adapted observer (exposure 75).
  - The stylized palette's arbitrary scale happens to land 1.7 stops under such an observer.

### 7.7 Which side is physically wrong [D]

- **Physical backend:** the radiance is right, to within the uncertainty of the cloud deck. The display is
  wrong: a unitless stylized constant (1.25) is applied through a field shared by both backends, and the
  authoring schema cannot express a physical value. **The black frame is the error.** The scene must not
  be brightened. Exposure must become an observer quantity in cd/m² (§6.9), or at least a per-backend
  value in consistent units.
- **Shader backend:** physically wrong radiometry.
  - No units, so no observer model can be driven by it.
  - Dusk to 31° spans 1.9 stops instead of 2.9.
  - Night is brighter than its own dusk and 14–18 stops too bright against real moonlight.
  - Its grey dusk reads close to what an adapted eye would see at this camera, **by authoring, not by
    physics**.
  - It cannot represent the approved real night unless it adopts the physical environment's radiance and
    the same observer.
- **Both:** the sun is placed by hand (§3.6), about 93° from where the 15:00 sun stood.

### 7.8 Other defects found [C/D]

1. binary16 HDR in absolute RU underflows at night (§0.5, §2.7). Every Rgba16Float history and
   G-buffer that stores radiance is affected the same way, e.g. `G0F/G1F/G2F`
   (`src/renderer.rs:3835-3837`).
2. The photometric meter uses Rec.709 on spectral samples and has a 36 cd/m² floor (§0.6).
3. Light powers are uncalibrated by 10²–10⁴× (§4.9). Colour comes from an sRGB fit, not Planck.
4. **Preset-dependent law.** Without ray query, the physical backend shades with `mesh.wgsl`'s constant
   sun, 1.74, and moon, 0.21. With ray query it uses the atmosphere. CLAUDE.md forbids presets from
   changing emitters or laws. The selector is hardware, not the preset name, and the forward path is
   the Low tier.
5. The day phases put impossible sun and moon directions into Act I (§3.6).
6. Fog ×1.10 at night is an unphysical day-phase multiplier (§5.4).
7. The test `camera_exposure_and_surface_energy_have_no_campaign_or_view_fill` hard-codes the ban on
   adaptation that the owner has lifted (`src/renderer.rs:11047-11069`). The stylized-daylight test
   comment "15:00 local solar time: about 7.5 degrees" is correct for the elevation, 7.41° on 21 Dec
   (`:9903-9905`), but does not check azimuth.

### 7.9 Recommended changes, in order, with gates

The ledger rows follow CLAUDE.md principle 2.

1. **Pre-exposed HDR.**
   - Write radiance × P (the previous frame's exposure) into every radiance target. Divide it back out in
     the meter and the crush.
   - Identical in both presets.
   - Test: a 3×10⁻⁹ RU sky round-trips through the HDR target with under 1 % error, and a 2.7×10⁴ RU
     solar disc does not overflow at day exposure.
2. **Photometric meter.**
   - Use the §1.2 weights for Y_p and Y_s; range 10⁻⁷–10⁷ cd/m².
   - Test against the §1.3 blackbody table and the solar table (±3 %).
3. **Exposure authority.**
   - Remove `camera_exposure` from the physical path, or re-express it as EV100 with per-backend
     conversion.
   - Test: an authored lighting record cannot change a physical-backend frame's exposure except through
     the observer.
4. **Ephemeris.**
   - Date, time, latitude and longitude produce the Sun and Moon. Delete the `DayPhase` light directions
     and `night_factor`.
   - Tests: Meeus 25.a and 47.a vectors; Espenak phases of Dec 1942 ±2 min; 15:00 LAT on 21 Dec →
     7.41° ±0.05°, compass 220.9° ±0.2°.
5. **The Moon and the night sky as emitters.**
   - The Moon: a second finite disc, radiance per §2.3 with the USNO phase law and distance scaling,
     through the same atmosphere.
   - Airglow and stars: an emitting layer (§2.4 triplet, van Rhijn).
   - Tests: a clear full Moon at 60° gives 0.15–0.25 lx horizontal. Moonless clear gives 0.4–1.0 mlx.
     Civil dusk (−6°) gives 2–4 lx and nautical dusk (−12°) 2–10 mlx under a clear sky, against the
     USNO targets.
6. **Calibrated emitters.**
   - Replace hand-set W/sr with photometric anchors and Planck spectra (§4.10).
   - Flares: parachute bodies with burn curves and descent (§4.1–4.3).
   - Fires: area emitters at 1,250–1,350 K.
   - Muzzle flash: a volume at 1,500 K for 65 ms (§4.6).
   - Tests: LC 50 at 300 m gives 6.7 lx ±5 % directly below; a burning house of 5×10⁴ cd gives 20 lx at
     50 m.
7. **Cloud–snow coupling for local light** (fire-glow, §2.5). Test: 20 × 5×10⁴ cd under a τ ≈ 49 deck
   over snow gives 0.1–0.5 lx at the ground, versus about 0.01 lx without the deck.
8. **Media.** Spectral snow ω (§5.2); smoke ω/g pairs (§5.3); delta-scaled flake extinction.
9. **Observer model (§6.9).**
   - Replace the banned-adaptation test with:
     - (a) the observer never writes transport buffers;
     - (b) identical constants in High and Laptop;
     - (c) time-course tests: cone plateau under 10 min, rods at 30–40 min to within 0.5 log units of
       absolute threshold; flash recovery per §6.6;
     - (d) the display mapping is monotonic and a moonless scene maps under 2 % of display white.
   - Ledger row: "Observer (eye) model: reproduction transform, not physics", mirroring the audio
     "Reproduction transform (not physics)" row (`docs/PHYSICAL_TRUTH_AUDIT.md:99`).
10. **Owner decisions.**
    - Veiling glare (§6.8).
    - Whether the shader backend adopts physical radiance or remains a daytime-stylized path that cannot
      claim a physical night.

---

## 8. Gaps and open items

- **Not retrievable this session** (paywall or 403):
  - Watson & Yellott 2012 (full text); Ferwerda et al. 1996; Krisciunas & Schaefer 1991;
  - Rasmussen et al. 1999 (snowfall visibility); US FMH-1 (403);
  - NUREG-1805 and the SFPE Handbook (fire correlations); CIE 146 and 191; ISO 834;
  - Ellis 1981 (page scans); Kyba 2017 was read only through a summariser;
  - OP 1666 vol. 2 (Internet Archive 500 error).
- Every constant from these is tagged [R] and needs checking before shipping.
- **No photometry of German 26.7 mm or Soviet 26 mm signal and illumination cartridges, Soviet SAB
  flares or Soviet illuminating shells** was found. The values are [E], bracketed by German aircraft flares
  (TM 9-1985-2) and US 1943 data (TM 9-1981). Russian sources (e.g. Soviet *Nastavlenie* manuals) should
  be searched when the search budget allows.
- The **8.8 cm KwK 36 flash** is scaled from a modern 152 mm howitzer (Steward 2011). The WWII
  diglycol propellant's flash tendency and the effect of the KwK 36's muzzle brake are unknown.
- **Daily weather** (cloud, snowfall, temperature) for Stalingrad nights Dec 1942 – Jan 1943 was not
  researched. It decides whether the moonlight in §3.5 reached the ground. The *Kriegstagebuch* or
  Soviet meteorological records are the sources to seek.
- **Rod regeneration** constants are classical [R] and derived [D], not re-measured.
- **The flash-recovery fit** is extrapolated below 9×10⁵ td·s, the range where muzzle flashes sit.
- **Mesopic appearance** (colour loss, acuity) beyond CIE 191's luminance weighting rests on published
  models I could not fetch this session (Kirk & O'Brien 2011; Wanat & Mantiuk 2014).

---

## 9. Parameter block (YAML)

```yaml
light_night_adaptation:
  meta:
    report: LIGHT_NIGHT_ADAPTATION_20260929.md
    date: 2026-09-29
    repo_commit_read: c609667e3d73e1a88fd74da61a68b05fa7832410
    tags: {M: measured, S: standard_fetched, R: standard_recalled_verify, D: derived, E: estimate, C: code_fact}
  units:
    renderer_unit: "W m-2 sr-1 nm-1 spectral radiance at 680/550/440 nm (Bruneton model.h)"
    band_wavelengths_nm: [680, 550, 440]
    photopic_weights_cd_per_RU: {r680: 12458.6, g550: 52558.9, b440: 7965.8, sum: 72983.3, tag: D, src: "CVRL vl1924e_1 piecewise-linear"}
    scotopic_weights_scd_per_RU: {r680: 2322.5, g550: 89437.1, b440: 73261.6, sum: 165021.2, tag: D, src: "CVRL scvle_1"}
    weight_read_error_vs_true: {sun: 0.987, bb5800K: 0.988, bb2800K: 1.011, bb1500K: 1.42, bb1300K: 1.70, bb1200K: 1.92, tag: D}
    solar_toa_illuminance_lx: {value: 132538, tag: D, src: "Bruneton kSolarIrradiance x CIE V"}
    blackbody_2041K_luminance_cd_m2: {value: 5.97e5, check: 6.0e5, tag: D}
    ev100: {formula: "EV100 = log2(L*100/12.5)", tag: S, src: "Frostbite PBR v3.2"}
    fp16: {max: 65504, min_normal: 6.104e-5, min_subnormal: 5.96e-8, tag: S}
    fp16_floors_cd_m2_in_RU: {normal: 4.5, subnormal: 0.0044, tag: D}
    physical_default_exposure_320_midgrey_cd_m2: {value: 41, ev100: 8.4, tag: D}
  site:
    lat_deg: 48.708
    lon_deg_east: 44.513
    engine_axes: "+X east, +Y up, +Z south (src/terrain.rs:4)"
    engine_sun_direction: "(cos(el)cos(az_e), sin(el), cos(el)sin(az_e)); az_e = compass_az - 90"
  clocks:
    german_CET_utc_offset_h: {value: 1, valid: "1942-11-02 .. 1943-03-29", tag: S, src: "tzdata C-Eur"}
    moscow_MSK_utc_offset_h: {value: 3, tag: S, src: "tzdata Europe/Moscow"}
    stalingrad_civil_utc_offset_h: {value: 4, tag: S, src: "tzdata Europe/Volgograd 1930-1961"}
    local_apparent_solar_noon_utc: {range: "08:47 .. 09:16", tag: D}
  sun:
    algorithm: "Meeus ch.25 low accuracy + apparent longitude; validated on Meeus 25.a"
    delta_t_s: 26
    solstice_noon_altitude_deg: {value: 17.85, tag: D}
    noon_altitude_deg: {"1942-11-19": 21.94, "1942-12-21": 17.85, "1943-01-15": 20.04, "1943-02-02": 24.28, tag: D}
    day_length_h: {"1942-11-19": 9.05, "1942-12-21": 8.27, "1943-01-15": 8.70, "1943-02-02": 9.47, tag: D}
    lat_1500_on_1942_12_21: {utc: "12:00", altitude_deg: 7.41, compass_az_deg: 220.9, engine_az_deg: 130.9, engine_dir: [-0.649, 0.129, 0.750], tag: D}
    lat_1500_on_1943_01_15_sun_at_7p5deg: {utc: "12:27", compass_az_deg: 225.3, engine_az_deg: 135.3, tag: D}
    courtyard_authored: {elev_deg: 7.5, engine_az_deg: 38, compass_az_deg: 128, verdict: "morning direction; ~93 deg off 15:00", tag: C}
  twilight_model_usno171:
    src: "USNO Circular 171 (Janiczek & DeYoung 1987), DTIC ADA182110; fits Brown 1952 (12,000+ measurements)"
    tag: S
    refraction: "HA = h + 1/tan((h+8.6/(h+4.42)) deg)/60 for h >= -5/6"
    airmass: "X=753.66156; S=asin(X cos HA/(X+1)); m = X(cos S - sin HA) + cos S"
    M: "exp(-0.21 m) sinHA + 0.0289 exp(-0.042 m) (1 + (HA+90) sinHA/57.29578)"
    E_sun_lx: "133775 * M / SK"
    night_sky_term_lx: {code: 0.0005, text: 0.0003}
    sky_condition_divisor_SK: {clear_lt_70pct: 1, thin_cloud: 2, average_cloud: 3, dark_stratus: 10}
    table_clear_lx: {"60": 102960, "35.4": 62156, "31": 53359, "17.85": 26182, "10": 11231, "7.5": 7329, "5": 4252, "0": 984, "-0.833": 662, "-3": 66.4, "-6": 2.99, "-9": 0.116, "-12": 0.0044}
  moon:
    algorithm: "Meeus ch.47 full series; validated on Meeus 47.a; phases match Espenak to 1 min"
    illuminance_model_usno171: "H = h - 0.95 cos h; P = 0.892 exp(-3.343/tan(E/2)^0.632) + 0.0344 (sin E - E cos E); P = 0.418 P/(1 - 0.005 cos E - 0.03 sin h); E_moon = P*M(H)/SK"
    cross_check_krisciunas_schaefer: {V_alpha: "-12.73 + 0.026|a| + 4e-9 a^4", E_toa_fc: "10^(-0.4(V+16.57))", tag: R}
    full_moon_toa_lx_alpha2deg: {range: [0.30, 0.33], tag: D}
    disc_solid_angle_sr_mean: 6.42e-5
    disc_luminance_full_cd_m2: {range: [4650, 5190], tag: D}
    disc_radiance_full_RU: {r680: 0.067, g550: 0.069, b440: 0.057, tag: "D (lunar reflectance slope 0.80/1.00/1.22 is E)"}
    measured_full_moon_lx: {max_supermoon_zenith: 0.30, measured_vienna_2016: 0.26, midlatitude_typical: [0.05, 0.2], tag: M, src: "Kyba et al. 2017"}
    moonlit_clear_sky_cd_m2_full: {value: 0.018, tag: D}
    declination_range_deg_1942_43: 19.3
    winter_full_moon_culmination_deg: [58, 60]
    phases_utc:
      new: ["1942-11-08 15:19", "1942-12-08 01:59", "1943-01-06 12:38", "1943-02-04 23:29"]
      first_quarter: ["1942-11-15 06:57", "1942-12-14 17:47", "1943-01-13 07:49", "1943-02-12 00:40"]
      full: ["1942-11-22 20:24", "1942-12-22 15:03", "1943-01-21 10:48", "1943-02-20 05:45"]
      last_quarter: ["1942-12-01 01:37", "1942-12-30 18:37", "1943-01-29 08:13", "1943-02-27 18:23"]
      tag: M
      src: "NASA/Espenak Six Millennium Catalog"
    max_clear_moon_lx_in_dark_hours: {"1942-11-19": 0.068, "1942-11-23": 0.159, "1942-12-12": 0.003, "1942-12-16": 0.032, "1942-12-19": 0.090, "1942-12-22": 0.222, "1942-12-24": 0.111, "1943-01-10": 0.002, "1943-01-16": 0.058, "1943-01-21": 0.213, "1943-01-22": 0.137, "1943-01-26": 0.032, "1943-01-31": 0.001, "1943-02-02": 0.0, tag: D}
    moonless_windows: ["1942-12-04 .. 1942-12-11", "1943-01-02 .. 1943-01-09", "1943-01-31 .. 1943-02-07"]
  night_sky:
    zenith_mag_arcsec2: {U: 22.0, B: 22.7, V: 21.9, R: 21.0, I: 20.0, tag: M, src: "Benn & Ellison 1998 La Palma, sunspot minimum"}
    brightening_mag: {low_ecliptic_latitude: 0.4, solar_maximum: [0.4, 0.5], airmass_1p5: 0.25}
    luminance_cd_m2: {falchi_22p0: 1.74e-4, benn_ellison_v21p9: 1.9e-4, tag: M}
    radiance_RU: {r680: 3.4e-9, g550: 2.7e-9, b440: 2.3e-9, tag: D}
    scotopic_to_photopic: 2.1
    van_rhijn_layer_height_km: 90
    horizontal_illuminance_lx: {range: [0.0005, 0.0008], tag: "M/S"}
  overcast_night:
    natural_darkest_zenith_cd_m2: {value: 7.0e-5, tag: M, src: "Jechow & Holker 2019"}
    natural_horizontal_lx: {range: [3.0e-5, 2.0e-4], tag: D}
    snowglow_amplification: {snow_and_clouds_vs_clear: 188, snow_vs_overcast: 33, rural_overcast_snow_vs_clear: 26, suburban_max_lx: 0.79, tag: M}
    fireglow_under_deck_lx: {range: [0.05, 0.5], model: "E = R_c F_up/(1 - R_c A); F_up = Phi (1+A)/2 T_smoke/S", tag: "D/E"}
    cloud_base_reflectance_tau49: {value: 0.8, tag: D}
  snow_surface:
    visible_albedo_pure_r100um: {clear: 0.9849, overcast: 0.9856, tag: M, src: "Dang et al. 2015"}
    all_wave_albedo_r100um: {clear: 0.834, overcast: 0.899, tag: M}
    ice_absorption_coeff_m: {nm440: 0.0018, nm550: 0.0523, nm680: 0.386, tag: M, src: "Warren & Brandt 2008"}
    asymptotic_formula: {A: "exp(-4 sqrt((1-w)/(3(1-g))))", g: 0.89, B: 1.5, bc_mac_m2_g: 7.5, tag: "D (constants R)"}
    albedo_table_550: {fresh_r100: 0.984, old_r500: 0.965, sooted_r250_1000ngg: 0.789, burnt_r500_10000ngg: 0.348, tag: D}
    recommended: {fresh_visible: [0.95, 0.98], trampled_sooty: [0.35, 0.8]}
  sources:
    - {id: lc50f_4candle, intensity_cd: 600000, burn_s: 300, descent_m_s: [3.5, 5.5], release_alt_m: [1500, 2000], spectrum_K: 3000, triplet_RU: [14.1, 7.68, 2.65], tag: "M (cp, burn), E (descent)", src: "TM 9-1985-2"}
    - {id: fb50_single, intensity_cd: 216000, burn_s: 300, triplet_RU: [5.07, 2.76, 0.953], tag: M, src: "TM 9-1985-2"}
    - {id: us_an_m26, intensity_cd: 800000, burn_s: [180, 210], descent_m_s: 3.5, tag: M, src: "TM 9-1981"}
    - {id: soviet_sab, intensity_cd: [2.0e5, 1.0e6], burn_s: [180, 360], descent_m_s: [3, 6], tag: E}
    - {id: star_26mm_white, intensity_cd: [1.0e4, 2.0e4], burn_s: [6, 8], apex_m: [80, 100], triplet_RU_2e4: [0.518, 0.251, 0.074], tag: E}
    - {id: us_m17_star_parachute, intensity_cd: 20000, burn_s: [20, 30], descent_m_s: 2.1, apex_m: 180, tag: M, src: "TM 9-1981"}
    - {id: us_60mm_illum, intensity_cd: 145000, burn_s: 25, descent_m_s: 3.0, tag: M, src: "TM 9-1981"}
    - {id: us_trip_flare_parachute, intensity_cd: 110000, burn_s: 20, height_m: [90, 150], lit_radius_m: 275, tag: M}
    - {id: flak_searchlight_150cm, intensity_cd: 9.9e8, tag: M, src: "TM-E 30-451"}
    - {id: burning_house, intensity_cd: [1.5e4, 1.5e5], central_cd: 5.0e4, flame_T_eff_K: [1250, 1350], flame_luminance_cd_m2: [300, 1500], flame_height_m: [10, 20], triplet_RU_central: [5.40, 0.333, 0.0066], sp: 0.47, tag: "D/E"}
    - {id: embers_ruin, luminance_cd_m2: [0.2, 23], T_K: [900, 1100], tag: D}
    - {id: burning_tank, intensity_cd: [5.0e3, 3.0e4], central_cd: 1.5e4, hrr_mw: [5, 15], flame_height_m: [5, 10], tag: E}
    - {id: muzzle_88_kwk36, intensity_cd: [2.0e4, 1.0e5], duration_s: 0.065, area_m2: [8.5, 15], core_T_K: [1450, 1550], visible_emissivity: [0.2, 0.5], ir_fraction: 0.99, tag: "M (152mm anchor) scaled D/E", src: "Steward 2011 AFIT"}
    - {id: muzzle_76_f34, intensity_cd: [1.0e4, 5.0e4], duration_s: 0.05, area_m2: [5, 8.5], tag: E}
    - {id: tracer_small_arms, intensity_cd: [300, 1000], duration_s: 2, colour: red, tag: E}
    - {id: tracer_88_base, intensity_cd: [1.0e4, 5.0e4], duration_s: [2, 4], tag: E}
    - {id: pressure_lamp, intensity_cd: [300, 500], T_K: 2800, tag: E}
    - {id: stove_door, intensity_cd: [2, 70], T_K: [1100, 1300], tag: D}
  game_lights_as_read:
    own_muzzle: {code: "420*(1,.74,.40) src/game.rs:16945", reads_cd: 2.3e7, overbright_x: [230, 1100]}
    enemy_muzzle: {code: "220*(1,.70,.34) src/game.rs:17044", reads_cd: 1.1e7}
    burning_wreck: {code: "48*(1,.30,.08) src/game.rs:17034", reads_cd: 1.4e6}
    preset_fire: {code: "95 @1850K src/environment_authoring.rs:91", reads_cd: 2.0e6}
    tableau_lamp: {code: "5 @2800K catalog", reads_cd: 1.7e5}
    tableau_stove: {code: "2.5 @1850K catalog", reads_cd: 6.0e4}
  media:
    koschmieder_factor: 3.912
    mor_factor: 2.996
    snow:
      extinction_formula: "beta = Q 1.5 S/(v rho_s D)"
      beta_per_mm_h_typical: 2.8e-3
      game_sigma_t: {value: 0.0068, mor_m: 441, equivalent_mm_h: 2.4}
      single_scattering_albedo: {nm440: 0.999998, nm550: 0.99995, nm680: 0.9996, tag: D}
      asymmetry: {range: [0.75, 0.89], tag: R}
      diffraction_fraction_delta_scaling: 0.5
      visibility_classes_m: {light: ">800", moderate: "400-800", heavy: "<=400", tag: R, src: "FMH-1"}
    smoke_wood:
      mass_scattering_m2_g: [2.8, 4.2]
      mass_absorption_m2_g: [1.0, 1.4]
      ssa: {flaming: 0.75, smouldering: 0.9}
      asymmetry: {range: [0.44, 0.63], aged: 0.63}
      tag: M
      src: "Reid et al. 2005"
    smoke_soot_black:
      mass_extinction_m2_kg: 8700
      ssa: 0.25
      asymmetry: 0.6
      visibility_C: {reflecting: 3, emitting: 8}
      tag: "S (FDS) / R"
    haze_game: {ssa: 0.90, asymmetry: 0.80, scale_height_m: 1200, tag: C}
  eye:
    mesopic_range_cd_m2: {range: [0.005, 5.0], tag: R, src: "CIE 191:2010"}
    cie191_m: {formula: "m = 0.767 + 0.3334 log10(L_mes)", V0: 0.40200, tag: R}
    duplicity_cd_m2: 0.03
    absolute_threshold_cd_m2_40min: 1.0e-5
    peak_nm: {photopic: 555, scotopic: 507}
    sp_ratio: {sun_moon: 2.35, night_sky: 2.1, flare_3000K: 1.48, lamp_2800K: 1.39, stove_1850K: 0.85, flash_1500K: 0.61, fire_1300K: 0.47, embers_1100K: 0.33, tag: D}
    pupil:
      formula_stanley_davies: "D = 7.75 - 5.75 x/(x+2), x = (L a M/846)^0.41; M=1 binocular, 0.1 monocular"
      formula_age: "D_U = D_SD + (y - 28.58)(0.021323 - 0.0095623 D_SD)"
      tag: R
      src: "Watson & Yellott 2012"
      latency_s: {range: [0.12, 0.14], tag: M}
      constriction_full_s: {value: 1.0, tag: M}
      dilation_tau_s: {fast: 2.0, slow: 15.0, tag: E}
      dark_area_mm2: {value: 50, tag: M}
    light_adaptation:
      neural_s: {range: [0.02, 0.2], tag: M, src: "Mantiuk et al. 2011"}
      rod_pool_arcmin: 10
      tvi_ferwerda_1996: {tag: R, cones: "-0.72 | (0.249 l + 0.65)^2.7 - 0.72 | l - 1.255 (breaks -2.6, 1.9)", rods: "-2.86 | (0.405 l + 1.6)^2.18 - 2.86 | l - 0.395 (breaks -3.94, -1.44)"}
    dark_adaptation:
      rod_cone_break_min: [5, 8]
      absolute_threshold_reached_min: 40
      cones: {model: rate_limited, K_m: 0.2, v_per_min: [0.47, 0.50], sigma_inv_cd_m2_min_dilated: 710, sigma_inv_td_s: 2.1e6, exp_tau_min: [1.02, 1.04], recovery_min: 4, tag: M, src: "Mahroo & Lamb 2004"}
      rods: {model: rate_limited, K_m: 0.2, v_per_min: 0.03, classical_tau_s: 400, sigma_inv_scot_td_s: 1.0e7, tag: "R/D"}
      bleach_threshold_log_units_per_unit_bleach: {rods: 20, cones: 3, tag: M, src: "Webvision"}
      rod_saturating_scot_td: 1700
    flash_blindness:
      data: {energy_td_s: [9.0e5, 3.0e7], recovery_s: [20, 130], target: "20/60 at 0.06 mL", tag: M, src: "DTIC AD0840277"}
      fit: "log10 t = log10 20 + 0.53 log10(E/9e5) - 0.4 log10(L_target/0.19)"
      min_recovery_s: 0.2
      infrared_effect: none
      gunner_own_88: {energy_td_s: [6.0e3, 3.0e4], recovery_s_moonlit: [2, 5], recovery_s_dark: [24, 60], tag: "D/E"}
    point_source_threshold_lx: {value: 1.0e-8, tag: D}
    veiling_glare: {formula: "L_veil = 10 sum(E_i/theta_i^2)", tag: R, src: "CIE 146:2002", status: owner_decision}
  observer_model:
    placement: "after transport, before bloom/crush; never writes transport buffers; same constants in both presets"
    meter: {weights: photometric_and_scotopic, pyramid: "64x36 log", weights_fovea_20deg_global: [0.5, 0.3, 0.2], clamp_cd_m2: [1.0e-7, 1.0e7]}
    display_adaptation_cd_m2: 100
    mapping: "Ld = m s_c Yp (chromatic) + (1-m) s_r Ys (achromatic); s = TVI_c(Lda)/TVI(A)"
    pre_exposure: "P = 0.18/(L_a/72983) RU, clamp so brightest*P < 6.0e4"
    expected_display_fraction_of_white: {photopic: 0.5, full_moon: 0.07, quarter_moon: 0.05, moonless_clear: 0.006, moonless_overcast: 0.003, tag: D}
  code_findings:
    exposure_shared_field: "src/renderer.rs:5483, 7002"
    authoring_range: "src/environment_authoring.rs:321, 353 (0.02..8)"
    physical_default: "src/renderer.rs:754 (320)"
    shader_default: "src/renderer.rs:7002 (1.25)"
    night_floor: "src/renderer.rs:789-799, 809-816, 6936"
    stylized_night_palette: "src/renderer.rs:9878-9888; shaders/stylized_mesh.wgsl:383-392, 528-531"
    forward_moon: "shaders/mesh.wgsl:358-360"
    meter: "shaders/exposure.wgsl:26-32, 45, 97-101, 121"
    hdr_format: "src/renderer.rs:424 (Rgba16Float)"
    adaptation_ban_test: "src/renderer.rs:11047-11069"
    six_stop_record: "docs/URBAN_INTRO_20260928.md:295-298 (0.0008 vs 0.051)"
    physical_scene_mean_cd_m2: {range: [150, 175], tag: D}
    stops_physical_below_own_calibration: 8.0
    stops_physical_below_adapted_observer: 5.9
    stops_night_floor_too_bright: [14.4, 22.4]
```

---

## 10. Sources

**Fetched this session**
- USNO Circular 171: P. M. Janiczek & J. A. DeYoung, *Computer Programs for Sun and Moon Illuminance
  with Contingent Tables and Diagrams* (1987). DTIC ADA182110 via Internet Archive
  (`archive.org/download/DTIC_ADA182110/DTIC_ADA182110_djvu.txt`). Appendix B quotes D. R. E. Brown,
  *Natural Illumination Charts*, BuShips Report 374-1 (1952).
- NASA/Espenak, *Six Millennium Catalog of Phases of the Moon*, 1901–2000
  (astropixels.com/ephemeris/phasescat/phases1901.html).
- IANA tz database, `europe` file (github.com/eggert/tz).
- J. Meeus, *Astronomical Algorithms*, ch. 25, 47, 12, 15. Lunar tables from `soniakeys/meeus`
  `moonposition.go` (MIT).
- CIE 1924 V(λ) and CIE 1951 V′(λ), 1 nm: CVRL, UCL (`cvrl.org/database/data/lum/`).
- E. Bruneton, *Precomputed Atmospheric Scattering* (2017), `atmosphere/model.h`, `model.cc`,
  `demo/demo.cc` (github.com/ebruneton/precomputed_atmospheric_scattering).
- C. Kyba, A. Mohar & T. Posch 2017, "How bright is moonlight?", *A&G* 58(1):1.31, via WebFetch summary.
- F. Falchi et al. 2016, *Sci Adv* 2:e1600377, PMC4928945.
- A. Jechow & F. Hölker 2019, "Snowglow", *J Imaging* 5:69, doi:10.3390/jimaging5080069, PMC8320966.
- A. Jechow et al. 2017, *Sci Rep* 7:6741, doi:10.1038/s41598-017-06998-z, PMC5532222.
- M. Spitschan et al. 2016, *Sci Rep* 6:26756, doi:10.1038/srep26756, PMC4895134 (via PubMed).
- C. R. Benn & S. L. Ellison 1998, *La Palma night-sky brightness*, ING
  (ing.iac.es/astronomy/observing/conditions/skybr/skybr.html).
- C. Dang, R. E. Brandt & S. G. Warren 2015, *JGR Atmos* 120, doi:10.1002/2014JD022646
  (atmos.uw.edu/~sgw/PAPERS/2015_Snow_BC_parm.pdf).
- S. G. Warren & R. E. Brandt 2008, ice optical constants (atmos.uw.edu/ice_optical_constants/).
- TM 9-1985-2, *German Explosive Ordnance (Bombs, Fuzes, Rockets, Land Mines, Grenades and Igniters)*
  (1953), pp. 65–78, Internet Archive.
- TM 9-1985-3, *German Explosive Ordnance (Projectiles and Projectile Fuzes)* (1953), Internet Archive.
- TM 9-1981, *Military Pyrotechnics* (1943), Internet Archive.
- TM-E 30-451, *Handbook on German Military Forces* (1 Mar 1945), Internet Archive.
- AMCP 706-185, *Military Pyrotechnics Series Part One* (1967), DTIC AD0817071.
- *German Infantry Weapons* (MIS, 1943), Internet Archive (checked; no cartridge photometry).
- DA Pam 20-236, *Night Combat* (1953), and DA Pam 20-291, *Effects of Climate on Combat in European
  Russia* (1952), Internet Archive.
- B. J. Steward, *Characterization and Discrimination of Large Caliber Gun Blast and Flash Signatures*,
  AFIT/DS/ENP/11-D01 (2011), DTIC ADA552974.
- *A Mathematical Model of Flashblindness* (1965), DTIC AD0627332.
- *A Review of Research on Flash Blindness* (1968), DTIC AD0840277.
- *Compilation of Nuclear Test Flash Blindness…* (1971), DTIC AD0742837 (context only).
- O. A. R. Mahroo & T. D. Lamb 2004, *J Physiol* 554:417, doi:10.1113/jphysiol.2003.051250,
  PMC1664777.
- T. D. Lamb & E. N. Pugh 2004, *Prog Retin Eye Res* 23:307, doi:10.1016/j.preteyeres.2004.03.001
  (abstract via PubMed).
- A. B. Watson & J. I. Yellott 2012, *J Vis* 12(10):12, doi:10.1167/12.10.12 (abstract via PubMed).
- M. Kalloniatis & C. Luu, *Webvision*, "Light and Dark Adaptation" (webvision.med.utah.edu).
- R. Mantiuk et al. 2011, "HDR-VDP-2", *ACM TOG* 30(4) (cl.cam.ac.uk/~rkm38/pdfs/mantiuk11hdrvdp2.pdf).
- S. Lagarde & C. de Rousiers 2014, *Moving Frostbite to PBR*, course notes v3.2.
- J. S. Reid et al. 2005, *ACP* 5:827, doi:10.5194/acp-5-827-2005.
- NIST Fire Dynamics Simulator source, `read.f90` and `cons.f90` (github.com/firemodels/fds).
- de.wikipedia *Signalpistole*; ru.wikipedia *Сигнальный пистолет*, *Фотоосветительная авиабомба*.
- Project documents: `docs/URBAN_INTRO_20260928.md`, `docs/PHYSICAL_TRUTH_AUDIT.md`,
  `docs/PHYSICAL_ENVIRONMENT_TRANSPORT.md`, `docs/STALINGRAD_CAMPAIGN.md`, `docs/ENVSTORY.md`,
  `docs/research/SOUND_LEVELS_20260928.md`; workshop manifests under `assets/worldgen/workshops/`.

**Recalled, not fetched [R]**
- Krisciunas & Schaefer 1991 (PASP 103:1033).
- Ferwerda et al. 1996 (SIGGRAPH).
- CIE 191:2010; CIE 146:2002; ISO 834-1.
- Heskestad flame height; Babrauskas pool-fire rates; Tien et al. flame absorption (SFPE Handbook).
- Bond & Bergstrom 2006 (BC absorption).
- Kokhanovsky & Zege 2004 (snow optics).
- Rushton (rhodopsin kinetics).
- Allen, *Astrophysical Quantities*.
- US FMH-1 snow classes.
- WMO CIMO Guide (MOR).
- Zukoski puffing frequency.
- van Rhijn (1921).
- Macke et al. 1996 (ice phase functions).

---

## Appendix A — Sun, daily (UT; +1 h German, +3 h Moscow)

| Date | Sunrise | Noon | Noon alt | Sunset | Day | Civil end | Naut. end | Astro end |
|---|---|---|---|---|---|---|---|---|
| 1942-12-01 | 04:32 | 08:51 | 19.56 | 13:09 | 8.61 h | 13:45 | 14:24 | 15:02 |
| 1942-12-04 | 04:36 | 08:52 | 19.11 | 13:07 | 8.52 h | 13:44 | 14:23 | 15:01 |
| 1942-12-07 | 04:40 | 08:53 | 18.73 | 13:06 | 8.44 h | 13:43 | 14:23 | 15:01 |
| 1942-12-10 | 04:43 | 08:54 | 18.42 | 13:06 | 8.38 h | 13:43 | 14:23 | 15:01 |
| 1942-12-13 | 04:46 | 08:56 | 18.17 | 13:06 | 8.33 h | 13:43 | 14:23 | 15:01 |
| 1942-12-16 | 04:48 | 08:57 | 17.99 | 13:06 | 8.30 h | 13:43 | 14:24 | 15:02 |
| 1942-12-19 | 04:51 | 08:59 | 17.89 | 13:07 | 8.27 h | 13:44 | 14:25 | 15:03 |
| 1942-12-21 | 04:52 | 09:00 | 17.85 | 13:08 | 8.27 h | 13:45 | 14:25 | 15:04 |
| 1942-12-24 | 04:53 | 09:01 | 17.86 | 13:09 | 8.27 h | 13:47 | 14:27 | 15:05 |
| 1942-12-27 | 04:54 | 09:03 | 17.94 | 13:11 | 8.29 h | 13:49 | 14:29 | 15:07 |
| 1942-12-30 | 04:55 | 09:04 | 18.09 | 13:14 | 8.32 h | 13:51 | 14:31 | 15:09 |
| 1943-01-02 | 04:55 | 09:06 | 18.31 | 13:17 | 8.36 h | 13:53 | 14:34 | 15:12 |
| 1943-01-05 | 04:55 | 09:07 | 18.60 | 13:20 | 8.42 h | 13:56 | 14:36 | 15:15 |
| 1943-01-08 | 04:54 | 09:09 | 18.96 | 13:23 | 8.49 h | 14:00 | 14:39 | 15:18 |
| 1943-01-11 | 04:53 | 09:10 | 19.38 | 13:27 | 8.57 h | 14:03 | 14:43 | 15:21 |
| 1943-01-14 | 04:51 | 09:11 | 19.87 | 13:31 | 8.66 h | 14:07 | 14:46 | 15:24 |
| 1943-01-17 | 04:49 | 09:12 | 20.41 | 13:35 | 8.77 h | 14:11 | 14:50 | 15:28 |
| 1943-01-20 | 04:47 | 09:13 | 21.02 | 13:40 | 8.88 h | 14:15 | 14:54 | 15:31 |
| 1943-01-23 | 04:44 | 09:14 | 21.69 | 13:44 | 9.01 h | 14:19 | 14:58 | 15:35 |
| 1943-01-26 | 04:41 | 09:15 | 22.41 | 13:49 | 9.14 h | 14:23 | 15:02 | 15:39 |
| 1943-01-29 | 04:37 | 09:15 | 23.18 | 13:54 | 9.28 h | 14:28 | 15:06 | 15:43 |
| 1943-02-01 | 04:33 | 09:16 | 24.00 | 13:58 | 9.42 h | 14:32 | 15:10 | 15:47 |
| 1943-02-04 | 04:29 | 09:16 | 24.86 | 14:03 | 9.57 h | 14:37 | 15:15 | 15:51 |
| 1943-02-06 | 04:26 | 09:16 | 25.46 | 14:07 | 9.67 h | 14:40 | 15:18 | 15:54 |

Late November for comparison: 19 Nov rise 04:15, set 13:19, noon 21.94°, 9.05 h.

## Appendix B — Moon, nightly

Rise and set are searched from 07:12 UT on the date to 07:12 UT the next day ("--" means outside that
window). Brightness is the maximum during astronomical darkness, clear sky, USNO 171. Divide by 3 for
average cloud and by 10 for stratus.

| Evening of | Illum. 21 UT | | Moonrise | Moonset | Max alt (dark) | At UT | E max |
|---|---|---|---|---|---|---|---|
| 1942-12-01 | 42 % | waning | 21:14 | 09:57 | 43.3 | 02:40 | 0.011 lx |
| 1942-12-03 | 23 % | waning | 23:31 | 10:50 | 26.4 | 02:42 | 0.002 lx |
| 1942-12-05 | 7 % | waning | 01:59 | 11:47 | 5.7 | 02:44 | < 0.001 lx |
| 1942-12-07 | 0 % | — | 04:30 | 13:04 | below | — | 0 |
| 1942-12-09 | 5 % | waxing | 06:41 | 14:52 | below | — | 0 |
| 1942-12-11 | 20 % | waxing | 07:32 | 17:12 | 15.2 | 15:01 | 0.001 lx |
| 1942-12-13 | 41 % | waxing | 08:49 | 19:39 | 29.7 | 15:01 | 0.006 lx |
| 1942-12-15 | 62 % | waxing | 09:47 | 21:58 | 39.5 | 15:57 | 0.021 lx |
| 1942-12-17 | 80 % | waxing | 10:39 | 00:10 | 48.1 | 17:32 | 0.046 lx |
| 1942-12-19 | 93 % | waxing | 11:34 | 02:16 | 55.1 | 19:00 | 0.090 lx |
| 1942-12-20 | 97 % | waxing | 12:07 | 03:15 | 57.6 | 19:48 | 0.124 lx |
| 1942-12-21 | 99 % | waxing | 12:43 | 04:10 | 59.3 | 20:44 | 0.179 lx |
| 1942-12-22 | 100 % | full 15:03 | 13:25 | 05:01 | 60.2 | 21:01 | 0.222 lx |
| 1942-12-23 | 99 % | waning | 14:11 | 05:47 | 60.1 | 21:49 | 0.154 lx |
| 1942-12-24 | 96 % | waning | 15:03 | 06:27 | 59.1 | 22:37 | 0.111 lx |
| 1942-12-25 | 91 % | waning | 15:59 | 07:02 | 57.2 | 23:24 | 0.083 lx |
| 1942-12-27 | 77 % | waning | 17:59 | 07:33 | 51.3 | 00:52 | 0.045 lx |
| 1942-12-29 | 59 % | waning | 20:06 | 08:27 | 43.3 | 02:20 | 0.021 lx |
| 1942-12-31 | 39 % | waning | 22:21 | 09:18 | 32.6 | 03:00 | 0.007 lx |
| 1943-01-02 | 19 % | waning | 00:47 | 10:17 | 16.4 | 03:00 | 0.001 lx |
| 1943-01-04 | 4 % | waning | 03:14 | 11:37 | below | — | 0 |
| 1943-01-06 | 0 % | new 12:38 | 05:19 | 13:34 | below | — | 0 |
| 1943-01-08 | 8 % | waxing | 06:47 | 16:01 | 5.2 | 15:18 | < 0.001 lx |
| 1943-01-10 | 25 % | waxing | 07:21 | 18:33 | 25.1 | 15:20 | 0.002 lx |
| 1943-01-12 | 45 % | waxing | 08:17 | 20:54 | 41.1 | 15:22 | 0.012 lx |
| 1943-01-14 | 65 % | waxing | 09:10 | 23:06 | 50.5 | 16:18 | 0.030 lx |
| 1943-01-16 | 82 % | waxing | 10:09 | 01:08 | 56.8 | 17:44 | 0.058 lx |
| 1943-01-18 | 94 % | waxing | 11:23 | 02:57 | 60.0 | 19:18 | 0.102 lx |
| 1943-01-20 | 100 % | waxing | 12:58 | 04:27 | 59.5 | 21:05 | 0.201 lx |
| 1943-01-21 | 100 % | full 10:48 | 13:52 | 05:04 | 58.0 | 21:13 | 0.213 lx |
| 1943-01-22 | 98 % | waning | 14:51 | 05:37 | 55.6 | 22:06 | 0.137 lx |
| 1943-01-24 | 90 % | waning | 16:54 | 06:32 | 48.9 | 23:36 | 0.068 lx |
| 1943-01-26 | 75 % | waning | 19:02 | -- | 40.5 | 01:05 | 0.032 lx |
| 1943-01-28 | 55 % | waning | 21:17 | 07:49 | 31.8 | 02:40 | 0.013 lx |
| 1943-01-30 | 33 % | waning | 23:39 | 08:50 | 20.5 | 02:46 | 0.003 lx |
| 1943-02-01 | 14 % | waning | 01:58 | 10:15 | 5.1 | 02:43 | < 0.001 lx |
| 1943-02-03 | 2 % | waning | 03:53 | 12:17 | below | — | 0 |
| 1943-02-05 | 1 % | waxing | 05:16 | 14:48 | below | — | 0 |
