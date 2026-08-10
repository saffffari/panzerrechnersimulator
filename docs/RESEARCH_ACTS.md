# Kessel — ACT LOCATIONS & GEODATA (research pass, 2026-07-01)

> Product of a 4-agent verified research sweep. Full structured results: `docs/research/data/`
> (`geo.json`, `stalingrad.json`, `ballistics.json`, `acoustics.json`). Every URL below was
> verified reachable at research time; the marked files were **downloaded into `assets/`**
> (gitignored — they live on disk).

## Act 1 — Die Stadt: Stalingrad, winter 1942

We already own the 100 km DEM (`assets/terrain/stalingrad_100km_*`). The research adds the
**real landmark grid** (12 verified lat/lon anchors), the **frontline truth**, primary maps, and
winter imagery:

- **Landmarks (selection):** Grain Elevator 48.6880/44.4827 · Univermag (Paulus' HQ)
  48.7086/44.5165 · Mamayev Kurgan (Height 102.0) 48.7425/44.5369 · Stalingrad-1 station
  48.7125/44.5136 · Gerhardt's Mill 48.7155/44.5329 · Pavlov's House 48.7160/44.5315 ·
  Barrikady 48.7767/44.5841 · Krasny Oktyabr 48.7517/44.5625 · Tractor Factory (STZ)
  48.7938/44.6029 · central ferry 48.7065/44.5238 · Tsaritsa gully 48.6990/44.5250.
  Full list + battle roles: `stalingrad.json`.
- **Frontline, mid-Nov 1942 (Uranus eve):** Germans held ~90% of the city; 62nd Army compressed
  into three slivers nowhere >500 m deep (Rynok–Spartanovka pocket; the factories' eastern
  edges; the centre bridgehead around Pavlov's/the Mill). Detail text in `stalingrad.json` —
  this is the calibration for where Act 1's fighting/stealth density belongs.
- **Primary maps (downloaded to `assets/reference/maps/`):** Generalstab **Stadtplan von
  Stalingrad 1:20,000 (M-38-113)** via David Rumsey; **Stalingrad-Süd 1:25k 1942** (LoC);
  West Point city-operations map; Commons 13-Nov-1942 frontline. Plus Lage-Ost finding aids.
- **Winter imagery (downloaded):** Sentinel-2 winter pair 2024-01-17 →
  `assets/terrain/act1_rgb/volgograd_winter_S2_20240117_TCI_10m.tif` (+1024 preview).

## Act 2 — Die Heimkehr: the Oderbruch below the Seelow Heights (spring 1945)

**Location:** 52.531 N, 14.428 E — the polder floor east of Seelow, between Werbig, Golzow and
Sachsendorf, straddling Reichsstraße 1. **Why:** the Battle of the Seelow Heights (16–19 Apr
1945) — Zhukov's assault out of the Küstrin bridgehead across dead-flat, ditch-cut farmland
toward the 40–50 m scarp. Exactly the brief: open field-grid country, villages, the Alte
Oder/Hauptgraben drainage canals (our balka analogue — they genuinely stalled Soviet armor),
and the heights as the act's visual wall.

- **DEM (downloaded):** Copernicus GLO-30 tile `N52_00_E014_00` →
  `assets/terrain/act2/Copernicus_DSM_COG_10_N52_00_E014_00_DEM.tif` (2400×3600).
- **Spring imagery (downloaded):** Sentinel-2 L2A `S2C_33UVU_20250402` (0.01% cloud!), 10 m
  true-color crop 17.8×15.7 km around Seelow → `assets/terrain/act2/S2C_..._seelow_crop.tif`.
- **Maps (verified URLs in `geo.json`):** Messtischblatt 1:25k **3452 Seelow (1934)** — the
  survey the Wehrmacht fought from; **Soviet General Staff 1:50k "ZEELOV" 1945 wartime sheet**
  (the attacker's own cartography); AMS/GSGS 1952 reprint; Seelow operational battle map;
  West Point Oder-front atlas sheet.

## Act 3 — Kameraden: HALBE (the Kessel von Halbe, April 1945)

**Location:** 52.1065 N, 13.6963 E. **This is the find of the pass:** the game is named
*Kessel*, and the **Kessel von Halbe** is the literal, historical climax of the premise —
Busse's encircled 9. Armee (~80,000 men) breaking out **westward through the small town of
Halbe**, 24 Apr–1 May 1945, **spearheaded by the last operational Tigers of s.SS-Pz.Abt. 502**,
through Konev's blocking lines on the A13/Autobahn, point-blank tank fighting in the town
streets, pine forest and firebreaks all around. A small eastern-German-countryside town, a
last-stand tank battle before Berlin, a lone-heavy-tank-leads-the-breakout scenario — the
design's finale, verbatim, in the historical record. (Halbe today holds Germany's largest WWII
war cemetery.)

- **DEM (downloaded):** Copernicus GLO-30 `N52_00_E013_00` →
  `assets/terrain/act3/Copernicus_DSM_COG_10_N52_00_E013_00_DEM.tif`.
- **Spring imagery (downloaded):** Sentinel-2 `S2A_33UVT_20250404` (0.03% cloud), 10 m crop
  13.9×12.5 km around Halbe → `assets/terrain/act3/...halbe_crop.tif`.
- **Maps (verified):** Messtischblatt 1:25k **3848 Märkisch Buchholz, 1941 wartime edition**
  (covers Halbe: the rail line, forest blocks, firebreaks); **Großblatt 77
  Zossen–Beeskow–Lübben 1:100k, printed March 1945 — ONE MONTH before the battle**; GSGS Ed.2
  Apr 1946 reprint; Berlin-operation maps showing the pocket + breakout axis.

## What this changes in the design

1. `world.rs` acts 2/3 get real places: captions + landmark names now reference the Oderbruch/
   Seelow scarp and Halbe (town, rail, Autobahn blocking line, forest). Act-local layout
   coordinates remain gameplay-authored; the DEMs/imagery/maps drive the fine build-out.
2. The worldgen pipeline (`tools/worldgen/`) can source Act 2/3 heightfields from the
   downloaded GLO-30 GeoTIFFs (conversion step TBD) and use the TCI crops as texture/reference.
3. The historical maps are the *paper map* art reference — the in-game M-key map should read
   like the Messtischblatt/Stadtplan sheets the crews actually carried.

## Ballistics & acoustics data (for the engines already built)

- `ballistics.json`: 12 guns with shell-by-shell mass/velocity/filler + penetration anchors at
  100/500/1000/2000 m (with the German-vs-Soviet test-standard reconciliation: German ~50%
  'durchschlagend' @30°; Soviet DeMarre K=2400 PTP/PSP limits — fold both to one internal
  standard when calibrating `ballistics.rs`), 13 armor schemes with per-plate thickness/slope/
  material, and concrete model guidance (slope-effect exponents, normalization, shatter-gap,
  Lambert–Jonas params).
- `acoustics.json`: mass-law transmission loss for 25–80 mm steel (with coincidence dips),
  cavity-resonance + RT60 recipe for the crew compartment, the 4 kHz acoustic-trauma tinnitus
  model with duck/recovery envelopes, ISO 9613 air-absorption dB/km table (the distance filter
  for gun reports), and ILD/ITD panning parameters — all mapped to plain-Rust DSP. These refine
  the `acoustics.rs` implementations (which currently use sane first-pass constants).
