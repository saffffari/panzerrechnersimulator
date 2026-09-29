# Stalingrad city centre, January 1943: the urban fabric (R1)

Research for the urban-intro city team (docs/URBAN_INTRO_TEAM_20260928.md, role R1), 28 September 2026.
Scope: the building types, blocks, streets, yards and street furniture of the Stalingrad city centre, their
state in January 1943, the defensive changes made to them, and the facts that constrain a tank in those
streets. It ends with a machine-readable parameter table (`urban_fabric_parameters`) for the grammar (G) and
the district (L).

## How to read this document

Every number carries a source key in square brackets (`[S5 p. 464]`) that resolves in §11, and one of four
provenance tags:

- **[D] documented**: a source states it.
- **[M] measured**: I measured it from a cited primary map, dataset or photograph; the method and error are
  stated. Measurements are documented data about the object measured, not about the whole city.
- **[P] photo cross-check**: a measurement from the parallel photo analysis (`PHOTO_REFERENCE_ANALYSIS_20260928.md`,
  rule numbers given), made from the reference set in `/home/alex/panzer/reference/wartime_photos_20260928/`.
- **[I] inferred**: my reasoned estimate, with the reasoning shown. Inferred values are proposals for the game,
  not historical facts, and may be overruled by any better source.

Where sources disagree, both are given. Where nothing was found, the document says so.

Limits of this study (read before relying on it):

- WebSearch was unavailable (session budget exhausted); all web sources were reached by direct URLs, archive
  catalogues, Wikipedia/Wikisource APIs, OpenStreetMap and the photo set. Glantz & House, Beevor, Jason Mark
  and Wijers were not read directly; where they appear, they are cited through the article that quotes them.
- The 1942 German city plan was read at 1.7 m per pixel (the archive's image server timed out on full
  resolution), so block measurements carry about ±3-5 m.
- OpenStreetMap measurements describe surviving and rebuilt buildings today. Survivors are biased towards the
  sturdier houses and the less-fought districts, and many carry post-war storeys or annexes.
- No source was found that counts the pre-war buildings of the centre by type. All shares in §4 and §6.2
  are therefore inferred.

---

## 0. The answer in brief

1. **The centre was mixed, not uniform.** It held 1-2-storey pre-revolutionary brick houses in the Tsaritsyn
   "brick style", many raised to 3-4 storeys in the 1920s-30s, and 4-6-storey 1930s blocks along the main
   streets and squares. Wooden and mud-walled one-storey houses filled the back streets and the fringes. Yards
   were full of sheds, stores, stables, summer kitchens and workshops [D: S14, S10i, S10p, S5 p. 464, S13, S1].
2. **Plots and buildings.** Surviving pre-1917 brick houses are 9.5-63 m long (median 26.6 m, interquartile
   18-34 m), and their wings are 8.4-16.5 m deep (median 11.1 m) [M: S8, S9, N = 36]. 1930s blocks are
   12-16 m deep, 27-80 m long, and have 3-6 storeys [M/D: S8, S11, S10c, S14, S5 p. 453].
3. **Blocks.** On the German plan of June 1942, central blocks come in two sizes: about 95-120 × 47-75 m
   (half blocks) and 150-215 × 60-100 m (full blocks) [M: S6]. The old Zatsaritsyn grid has a street pitch of
   93-126 m by 184-199 m [M: S8].
4. **Streets.** Facade to facade, streets measure 17.6-29 m where pre-revolutionary buildings survive on both
   sides [M: S8, S9]; the 1942 plan draws most gaps at 9-22 m [M: S6]. Squares are 60-260 m across [P L3;
   D: S14].
5. **Trams.** The track gauge is 1524 mm, on heavy rails over pine sleepers and sand ballast. Steel tubular
   masts stand between the tracks, except in the centre, where the wire hangs from span wires on poles at both
   sides [D: S15 via Radtsig 1962]. The network was destroyed; wrecked tramcars stood in the streets in
   February 1943 [D: S5 p. 464; S3 p. 393].
6. **The state in January 1943.**
   - 41,685 houses (90.5 % of the pre-war housing stock) were destroyed city-wide [D: S17, S1]; S2 gives
     41,895 buildings.
   - On 2 February 1943 the Ermansky (central) district had 33 inhabitants [D: S2].
   - Brick buildings stood mostly as roofless, floorless shells. Wooden houses were reduced to "heaps of ash
     with stove chimneys sticking up" [D: S3 p. 105; S5 pp. 202, 464-465; S28].
   - Streets were strewn with broken brick, twisted rods, vehicle wrecks and more than 150,000 craters
     city-wide [D: S2].
7. **Defence.** Stone and brick buildings with cellars became strongpoints, linked by trenches and
   communication trenches. Fire came from semi-basements along the streets and from upper floors and attics
   down onto tanks. Windows and breaches were turned into embrasures, entrances were barricaded or bricked up,
   and approaches were mined and wired. Wooden, one-storey and mud-walled houses were ignored and became no
   man's land [D: S3 pp. 296-299, 157; S4 p. 320; S11; S13].
8. **The tank.**
   - A Tiger is 3.56-3.70 m wide [D: S25].
   - Its gun, limited to -8°/+15° [D: S25], cannot reach the third storey of a 1930s block from inside a
     20-24 m street [I: §8.3]. Nor can it reach a semi-basement window closer than about 15 m.
   - Rubble berms of 1-2 m line the streets. Outward-fallen walls leave slopes of 1.5-3 m, and whole collapsed
     facades form ridges that tanks could not pass [P G17; D: S5 pp. 205-206, 211].

---

## 1. Sources and methods

### 1.1 Primary and near-primary sources used

- **The German army city plan** `Stadtplan von Stalingrad`, 1:20,000 [S6].
  - Published by the Generalstab des Heeres, current to 1 August 1942, and "angefertigt nach einer
    Bildplanskizze im Juni 1942" (drawn from a photo-mosaic sketch of June 1942).
  - Its block outlines were measured from IIIF crops at 1.698 m per pixel. The scale was calibrated on the
    printed scale bar: 589 scan pixels per 500 m, so 0.849 m per scan pixel.
  - Its descriptive text gives 445,470 inhabitants in 1939 and notes the city's recent "freie Plätze, breite
    Straßen, Gärten und Parkanlagen" and its trams [D: S6].
- **The 1908 `План города Царицына`**, "100 саженей в дюйме" [S7].
  - The printed scale bar was calibrated at 1.0005 m per pixel: 853 px for 400 sazhen, 1 sazhen = 2.1336 m.
  - Blocks were segmented by colour.
- **Chuikov's memoir** `Сражение века` (1975) [S3], checked against the 1964 English edition [S4].
- **Werth's eyewitness account** `The Year of Stalingrad` (1946), from his visit of 3-5 February 1943 [S5].
- **The German reconnaissance air photographs** of August 1942 and March 1943 of one central quarter, as
  published by Царицын.рф [S28].
- **The official post-battle statistics**, via Kuznetsova (2003), from the Volgograd party archive and
  Chuyanov's notes [S1], and the city history site [S2].
- **The 188-record reference photo set** [S29], read for proportions only; the full photo analysis is P's [S30].

### 1.2 Secondary and modern sources used

- Russian Wikipedia articles on the buildings and squares of the centre. They carry references to Гуркин
  1963, Олейников et al. 2011, Радциг 1962 and Adam's memoir [S11, S13, S14, S15].
- Царицын.рф, the local architectural-history site of R. Shkoda. It holds building-by-building histories with
  1942/1943 air-photo comparisons [S10].
- The regional heritage register [S9] and OpenStreetMap geometry [S8].

### 1.3 Measurement methods

- **Footprints.** For each pre-1917 heritage-listed house with coordinates [S9], the OSM building polygon
  containing the point was taken [S8]. It was measured as a minimum-area rectangle. An "equivalent wing
  depth" was solved from the polygon's area and perimeter, as for a rectangle, so that L- and U-shaped houses
  give the depth of their wings.
- **Street widths.** The minimum polygon-to-polygon distance was measured between surviving pre-1917 houses
  with opposite-parity numbers on the same street.
- **Blocks on the 1942 plan.** Hatched block areas were segmented by the density of dark pixels, cleaned
  with a 5 px morphological opening, labelled into connected components, and measured with minimum-area
  rectangles. Only components that fill at least 0.80 of their rectangle were kept, which excludes merged
  blocks.
- **Street pitch.** The perpendicular offsets between parallel street centrelines were measured in the
  surviving old grid south of the Tsaritsa [S8].

---

## 2. The centre in 1942: what it was

- **District.** In 1942 the centre was the **Ermansky district** [D: S19].
  - Created on 2 April 1935 with 344 ha and 50,000 inhabitants; today's Central district.
  - It sat between the Tsaritsa gorge to the south, the railway to the west and the Volga bluff to the east,
    around Stalingrad-I station, the Square of Fallen Fighters and 9 January Square (today Lenin Square).
- **Layers.**
  - **The 1820 regular plan** [D: S18]. Tsaritsyn's regular plan was approved on 16 June 1820. Inside it,
    the irregular lanes of the former fortress quarter and the Preobrazhensky forshtadt survived at an angle
    to the grid [D: S10n, S10o].
  - **The boom of 1870-1915** [D: S17]. Population and trade grew more than tenfold. The fortress-era
    village of timber houses was rebuilt in brick "shops, hotels and income houses", in a local brick style
    with carved brick window heads (сандрики), half-columns and corbelled cornices [D: S10b, S10f].
  - **Additions in the 1920s-30s.** Many central houses received third and sometimes fourth storeys
    ("многие дома в центре города начали достраиваться третьими, а в отдельных случаях – и четвёртыми
    этажами") [D: S10i; also S10m, S10k].
  - **New buildings in the 1930s.** Old one- and two-storey houses in the centre were demolished and
    replaced by 4-5-storey buildings ("Старые одно-двухэтажные дома царицынской эпохи в центре города
    сносятся, их заменяют 4-5 этажными зданиями") [D: S10k].
- **Werth's view in February 1943.** He saw the centre as "one of the modern cities of Russia; its entire
  centre … had been built in the last ten or fifteen years; here were large blocks of flats — all burned out"
  [D: S5 pp. 464-465]. This describes the main streets. The Tsaritsyn-era houses behind them are documented
  by the sources above and are visible in the photographs [P L2; S29].
- **The city as a whole.** In 1941 the housing stock was 1,848,000 m² of living area:
  - municipal 308,000 m²;
  - departmental 720,000 m²;
  - private 820,000 m² (44.4 %), the private sector being overwhelmingly one-storey houses.

  Living space was 3.9 m² per person in 1940 [D: S1, Table 2].

---

## 3. Building types

The four contract typologies, `building_placement::Typology`, are kept. Section 3.5 lists what they leave
out.

### 3.1 merchant_brick_house (pre-revolutionary brick house, купеческий дом)

**What the sources say [D].**

- **Storeys and plinth.**
  - One or two storeys on a plinth or semi-basement (цокольный этаж) [D: S10e, S10g-S10q].
    - Kubanskaya 4: two storeys plus a non-residential semi-basement holding firewood stores, pantries and a
      laundry [D: S10e].
    - Rysin's house: two storeys plus a semi-basement with windows [D: S10h].
    - The Repnikova mansion: a high plinth of red brick; one tall main floor on the street and two storeys
      on the yard side, with a separate spiral metal stair [D: S22].
  - Raised to 3-4 storeys by the 1930s:
    - "Столичные номера" was raised to 4 storeys [D: S10i];
    - Lapshin's house, "partly two- and partly three-storey", was raised to four [D: S10m];
    - Yablokov's house was raised from 2 to 4 storeys [D: S10q].
- **Shops.** Shops occupied the street floor, and cellars served trade: Yablokov's "магазин с подвалами",
  1895 [D: S10q].
- **Street line and wings.** Houses were often "built into the common line of Tsaritsyn facades", with the
  building continuing deep into the yard [D: S10r], and gates of brick [D: S10g].
- **Brickwork.**
  - Red brick in Flemish bond dominated before about 1929; header bond was used for strong buildings such as
    the fire stations; English cross bond displaced Flemish from about 1929 [D: S10b].
  - Facades were often whitewashed [D: S10f].
  - The ground floor could be fully shuttered, with four shutter hooks per window [D: S10f].
- **Floors.** Floors were timber joists. The first Gerhardt mill building had "кирпичные стены и деревянные
  перекрытия" [D: S10l], and the 4-storey "Столичные номера" lost "внутренние деревянные перекрытия и крыша"
  while "все кирпичные части … присутствуют" [D: S10i].

**Measured sample [M: S8, S9].** 36 surviving pre-1917 masonry buildings in the central and Voroshilovsky
districts, today's footprints:

| quantity | min | 25 % | median | 75 % | max |
|---|---|---|---|---|---|
| long side (m) | 9.5 | 18.3 | 26.6 | 34.3 | 63.1 |
| short side of bounding rectangle (m) | 8.5 | 11.3 | 15.5 | 20.8 | 35.3 |
| equivalent wing depth (m), N = 30 | 8.4 | 9.8 | 11.1 | 12.0 | 16.5 |

- Present storey counts: 14 of one storey, 14 of two, 3 of three, 5 unknown.
- Median long side by storeys: 19.9 m for one-storey houses and 32.2 m for two-storey houses.
- Examples:
  - Kubanskaya 4 (1890s, 2 storeys plus semi-basement): 31.8 × 15.5 m.
  - Lapshin's house: 23.9 × 17.8 m.
  - Pushkina 13 (1890-1900, 2 storeys): 41.0 × 22.6 m, L-shaped.
  - Susokolov's house, Ostrovskogo 3 (1870s, 2 storeys, a third added in the 1960s): 29.1 × 20.4 m.
- **Window axes.** Susokolov's house has 11 windows on one facade and 7 on the other [D: S10j]. On a
  29.1 × 20.4 m footprint that gives axes of about 2.6-2.9 m [M, assuming the 11 windows are on the long
  side].

**Photo measurements [P].**

- Storey pitch 3.8-4.6 m on a plinth or semi-basement; a two-storey house stands about 9-10 m to its cornice
  [P G1].
- Upper windows about 1.1-1.4 × 1.9-2.2 m, often with segmental or round heads.
- Shop windows about 2.2-2.5 m and nearly square.
- Round-arched doorways about 1.25 m wide and 2.1 m to the crown.
- Yard gateways 2.5-3.5 m wide; sills about 0.9 m above the floor [P G2].
- My own reading of one small two-storey corner house in [S29: 07_residents-returning-sledge-feb-1943] gives
  eaves at about 6.5-7.5 m, so 3.2-3.7 m per storey (±15 %), and upper windows of about 1.0-1.1 × 1.8-1.9 m
  [M, scaled on the standing women]. The typical storey is therefore taken as 4.0 m, within a range of
  3.4-4.6 m.

**Inferred parameters [I].**

- **Wall thickness.** Brick counts are converted at 1 brick = 0.25 m, 1.5 = 0.38 m, 2 = 0.51 m and
  2.5 = 0.64 m [D: S21, «Каменные работы» 1987]. The 1927 Soviet brick of 250 × 120 × 65 mm is described as
  the national standard [D: S21, flagged "no authoritative source" on Wikipedia]. Pre-1927 bricks may be up
  to about 7 % longer [I].

  | wall | bricks | thickness |
  |---|---|---|
  | exterior, ground floor | 2-2.5 | 0.51-0.68 m |
  | exterior, upper floor | 2 | 0.51-0.55 m |
  | semi-basement | 2.5-3 | 0.64-0.77 m |
  | interior bearing | 1.5-2 | 0.38-0.51 m |
  | partitions | timber-framed and plastered, or ½ brick | 0.10-0.15 m |

  Photographs read wall thickness at breaks as 0.4-0.65 m [P G3].
- **Roof.** Hipped or gabled, low pitch (about 15-25°), sheet iron on timber rafters, hidden behind a
  parapet or attic on the street facade: Ogaryova 6 has five attics, Profsoyuznaya 5a a central attic
  [D: S10f, S10d; P G4]. Kubanskaya 4 kept its 19th-century rafters [D: S10e].
- **Cellar.**
  - About 0.6 of houses have a semi-basement or cellar [I, from the documented examples and the trade use of
    cellars].
  - The semi-basement floor lies 1.0-1.8 m below the street, with 2.2-2.8 m headroom and small windows
    (about 0.8-1.2 × 0.5-0.8 m) whose sills are 0.2-0.4 m above the pavement [I]. Cellar headroom is
    2.0-2.4 m in the photographs [P G6].
  - Cellars are vaulted in brick or have timber ceilings [I; brick-vaulted cellar ceilings are documented in
    the 1920s Barrikadnaya 22, S10c].
- **Stairs.** A timber stair inside, or a back stair in a yard projection (Ogaryova 6: a risalit housing the
  stair to the upper floor [D: S10f]), or an external timber gallery with stair and balcony at the back
  (Komsomolskaya 10a: "деревянная пристройка с лестницей и балконом", gone by 1943 [D: S10o2]).

### 3.2 apartment_block (1930s multi-storey brick block, "сталинка" and constructivist types)

**What the sources say [D].**

- **Pavlov's House.** Four storeys and four entrances; the Oblpotrebsoyuz house at Penzenskaya 61 on
  9 January Square, mid-1930s, architect S. Voloshinov, built of "кирпич, дерево" (brick, wood)
  [D: S11 citing Гуркин 1963]. Its twin, the Sovkontrol house ("Zabolotny's house"), stood opposite; a rail
  spur to the mill ran between them [D: S11, S13].
- **Around 9 January Square** stood the two 4-storey twins, a 2-storey plastered "milk house" and a 4-storey
  UpolNarkomTyazhProm office [D: S13].
- **Around the Square of Fallen Fighters** stood [D: S14 citing Олейников et al. 2011]:
  - a 5-storey city communal-department house (1928);
  - the 4-storey 1st House of Soviets (raised);
  - a 4-storey constructivist hostel;
  - the Hotel "Bolshaya Stalingradskaya" (1935, 230 rooms);
  - the 4-storey L-shaped Univermag, whose top floor was destroyed on 23 August 1942 and in whose basement
    Paulus had his headquarters in January 1943.
- **Barrikadnaya 22 (about 1929).** Three storeys and six entrances. In the battle "прогорели крыша, все
  деревянные перегородки и перекрытия, была разрушена центральная угловая часть здания и торец … однако
  наружные капитальные стены, коробки подъездов и часть кирпичных перекрытий подвала уцелели" [D: S10c].
- **Height of the central blocks.** From the steppe Werth saw "large imposing blocks of five and six-storey
  buildings: these were the shells of buildings in central Stalingrad" [D: S5 pp. 452-453].
- **General 1930s-50s practice** [D: S20]:
  - exterior walls 2.5 bricks (65 cm), interior bearing walls 1-1.5 bricks (25-38 cm);
  - timber or composite floors in the early period;
  - ceilings of at least 3.0 m;
  - 2, 3 or 4 flats per landing;
  - substantial basements in larger buildings, while two-storey ones were usually built without;
  - pitched or hipped roofs on timber, covered with sheet iron or slate.

**Measured [M: S8].**

- Pavlov's House (rebuilt 1943 on the original walls; the 1985 memorial colonnade excluded): main body
  about 13.2-14.9 m deep, 41.7-56.4 m long including the end wing; OSM tags give a hipped metal roof.
- Other 1930s blocks:
  - Univermag 48.2 × 16.0 m;
  - Proletkultskaya 3 (1937, 4 storeys) 27.2 × 13.4 m;
  - Generalsky House (1939, 4 storeys, L-shaped) wings about 13.7 m deep;
  - House of Communal Workers (1936, 5 storeys, L-shaped) wings about 11.5 m deep.
- The equivalent wing depth of the seven dated 1925-1941 blocks with three or more storeys in OSM has a
  median of 13.7 m (interquartile 11.5-15.2 m).
- Four entrances in about 56 m give about 14 m per stair section [M/I].

**Photographs [P].**

- Storey pitch 3.0-3.5 m, with the ground floor raised about 0.8-1.2 m over a cellar [P G1].
- Windows about 1.3-1.45 × 1.45-1.6 m on axes of 2.7-3.4 m (0.8-0.95 of the storey pitch). Some blocks group
  tall windows of about 0.9-1.0 × 2.1-2.3 m [P G2].
- My own measurement of [S29: 04_gutted-apartment-block-through-arch_1943] gives a window axis of 0.91 of
  the storey pitch and near-square windows (width/height 0.9) [M]. That is about 3.0 m axes and
  1.33 × 1.49 m windows at a 3.3 m pitch.
- Some roofs are flat behind brick parapets 0.8-1.0 m high [P G4]. Concrete stairs survive the fires
  [P G7, G9].
- Some blocks have reinforced-concrete slabs, which stay while timber floors burn out [P G9].

**Inferred [I].**

- Floor-to-floor 3.2-3.5 m (typical 3.3 m).
- Walls:
  - exterior 2-2.5 bricks (0.51-0.64 m), thicker (2.5) at ground floor;
  - interior spine and stair walls 1-1.5 bricks (0.25-0.38 m);
  - partitions timber and plaster.
- Floors: timber joists in about 0.6-0.7 of residential blocks, reinforced concrete in the rest and in
  public buildings (the documented examples go both ways) [I]. Stairs and landings are concrete [P].
- Cellar in about 0.8-0.9 of blocks; floor 1.8-2.4 m below the street, headroom 2.0-2.5 m [I; P G6].
- Stair: two flights per storey, flights about 1.05-1.2 m wide, stair bay about 2.6-3.0 × 5-6 m [I].
- Entrance doors about 1.2-1.5 × 2.2-2.5 m [I]; ground-floor entrances up to about 2.5 m wide [P G2].
- Balcony doors about 2.4 m tall [P G2].

### 3.3 wooden_house (one-storey timber house, деревянный дом)

**What the sources say [D].**

- The outskirts were "composed chiefly of wooden houses" (General Talensky to Werth) [D: S5 pp. 201-202].
  "The greater part of the town, and certainly most of the wooden houses, had been burned down"
  (Pushkov, 1942) [D: S5 pp. 254-255].
- In the old centre wooden houses were rare by 1942, since the boom replaced them in brick:
  - around the Square of Fallen Fighters the 34th block's one-storey wooden buildings were replaced in the
    late 19th century by two-storey brick houses [D: S14];
  - the Yablokov wooden house was demolished in 1890 [D: S10q].
- The only 1870s timber houses in the city to survive both wars are Pugachyovskaya 11, 11a and 11b. They are
  railway staff houses built to one 1872 design and are called "unique for Stalingrad-Volgograd"
  [D: S10a]. Their footprints today, with post-war additions, are 22.2 × 14.6 m, 23.9 × 12.9 m (two storeys)
  and 23.2 × 12.0 m [M: S8].
- Mud-walled houses ("здания с глинобитными стенами") existed alongside wooden ones [D: S13]. The 1945
  private sector was mostly "из древесных отходов, глины и других материалов" [D: S1].
- Houses stood with sheds ("дом жилой с сараем") [D: S9].

**Photographs [P].**

- One storey on a low masonry plinth; eaves 3.0-3.4 m, ridge 5.0-5.5 m; gabled or hipped standing-seam
  sheet-iron roof at about 30° with fretwork bargeboards [P G1, G4].
- Picket fences 0.8-1.1 m and board fences 1.6-2.0 m [P L5].
- Burned timber quarters stand on plots of about 8-12 m frontage, with stove stacks at a 10-20 m pitch
  [P L2].

**Inferred [I].**

- Footprint 6-12 × 6-10 m (typical 8 × 8 m).
- Walls:
  - log (0.18-0.25 m) or timber frame with boarding (0.15-0.20 m), or wattle and daub or adobe (0.25-0.4 m);
  - plinth 0.4-0.8 m high of brick or stone.
- Floor-to-ceiling 2.6-3.0 m.
- Windows 0.8-1.0 × 1.2-1.5 m, on axes of 2.0-2.8 m, with shutters or surrounds.
- Door 0.8-0.9 × 1.9-2.0 m; yard gate 2.5-3.0 m.
- About half the houses have a root cellar (погреб) 1.5-2.2 m deep under the floor or in the yard.
- One brick stove and chimney per house, the stove body about 1.5-2 × 2 m [I]. A Russian stove's firebox
  alone could be up to 1.5 m wide and 1.2-1.4 m high [D: S21b citing *Русская изба*, 2004, p. 49]. The stack
  stands about 1 m above the ridge, 6-7 m in all [P G13].

### 3.4 workshop (single-storey brick workshop, store, garage, shed)

**What the sources say [D].**

- On the Gerhardt mill's plot: two wooden grain warehouses on Kazanskaya Street, a one-storey office, and
  garage boxes on Penzenskaya Street [D: S13].
- Timber storage sheds were built against high single brick yard walls [D: S10o].
- The first tram depot (1912) was redesigned in reinforced concrete [D: S15].
- Workshops appear among the combat buildings: the "гвоздильный завод" (nail works) was stormed [D: S3
  p. 307].

**Photographs [P].**

- Brick workshops at a factory edge have piers 6-8 m high and 1.0-1.3 m wide, with tall window bays over low
  sill walls [P G1, G3; S29: 04_red-army-men-in-ruined-workshop_1942-11].
- Yard sheds are timber post-and-plank structures; blank warehouse walls are about 3-4 m high [P G1, G7].
- Roofs are gabled or segmental on trusses [P G4].

**Inferred [I].** For the yard and street workshops of the centre, not factory halls:

- Footprint 6-16 m span × 8-40 m length.
- Eaves 3.5-5.5 m.
- Walls 1.5-2 bricks (0.38-0.51 m) with piers at 3.5-6 m.
- Windows 1.2-2.0 × 1.5-2.5 m; gates 3-4 m wide and 3-3.5 m high.
- Gabled or lean-to roofs of sheet iron or boards on timber (steel in larger sheds).
- Earth or concrete floor; a cellar only rarely (about 0.1).

### 3.5 What the four typologies leave out

These are documented, but no typology covers them. Give them bespoke blueprints or record them as
exclusions:

- Public and commercial landmarks, which have their own form [P G20]: the Univermag, the station, hotels,
  schools, the mill (reinforced-concrete frame with brick shell, 5 storeys, 47.0 × 22.4 m [D: S13; M: S8]).
- 2-storey wooden houses (Pugachyovskaya 11a).
- Mud-walled houses.
- "Semi-stone" houses, with a brick lower floor and a timber upper floor. These are a general Russian urban
  type [D: S21c]; they were not specifically documented for Tsaritsyn in this study.
- Tall single chimneys of larger buildings (9-15 m) [P G13].

---

## 4. Share and arrangement of types in a block

**Documented anchors [D].**

- **The 34th block** by the Square of Fallen Fighters was "densely built". Its late-19th-century two-storey
  brick houses mostly lasted until the 1940s [S14].
- **9 January Square** was lined by 4-storey 1930s blocks, a 2-storey plastered house and a 4-storey office.
  The mill's yard held timber warehouses, a one-storey office and garages [S13].
- **The main shopping street** leading to the Square of Fallen Fighters had "nearly all the houses … large
  modern blocks, solidly built" [S5 pp. 464-465].
- **The Preobrazhensky forshtadt**, in the air photograph of August 1942 [S28]:
  - blocks packed with small roofed buildings on the street lines;
  - yards filled with outbuildings and trees;
  - long 1930s slabs at the edge, some set in parallel rows.
- **Back yards.** Street buildings continued deep into their yards with annexes (Mishnin's house, "содержит
  значительное количество пристроек") [S10r]. Timber annexes, sheds, warehouses and summer kitchens stood in
  the yards [S10o, S10o2, S1, S9].

**Inferred block recipes [I].** The district generator should vary them by position.

| block type | where | street-frontage occupancy | frontage by typology | yard contents |
|---|---|---|---|---|
| core | faces the main street or square | 0.75-0.9 | apartment blocks 0.4-0.6, merchant houses 0.3-0.5, gates and walls 0.05-0.15, wooden 0-0.05 | courtyards 30-60 m; sheds, garages, one workshop |
| ordinary | old-town side streets | 0.6-0.8 | merchant houses 0.5-0.7 (often continuous, party walls), wooden 0.1-0.3, apartment block 0-0.2, gates, walls and fences 0.1-0.25 | per plot 1-4 sheds or stores, a summer kitchen, sometimes a workshop |
| fringe | edge of the district | 0.4-0.6 | wooden houses 0.6-0.9 on 8-12 m plots, small brick houses 0.1-0.3, fences for the rest | gardens, sheds, root cellars |

Main-building shares for the whole intro district [I]:

| typology | share by count | share by footprint area |
|---|---|---|
| merchant_brick_house | 0.35-0.45 | 0.35-0.45 |
| wooden_house | 0.25-0.40 | 0.10-0.20 |
| apartment_block | 0.08-0.12 | 0.25-0.35 |
| workshop | 0.10-0.20 | 0.10-0.15 |

Yard sheds are counted separately as dressing, at 1-3 per plot.

Reasoning:

- The brief's district is the centre, where brick dominated. City-wide, 44 % of living area was private
  single-storey housing [D: S1].
- The fringe must show the burned timber quarters [D: S3 p. 105; P L2].
- Apartment blocks are few but large.

**Arrangement rules [I, from D above].**

- Buildings stand on the street line, with no front gardens in the core.
- Merchant houses form continuous street walls with party walls. Gaps between them are closed by gates or
  high yard walls.
- Plots run back to back from the two long sides of a block.
- 1930s blocks are perimeter L or U forms around a courtyard, or free-standing slabs set back from the street
  in parallel rows [P L2].

---

## 5. Block and street pattern

### 5.1 Blocks [M]

| source and area | blocks measured | small or half blocks | large or full blocks |
|---|---|---|---|
| German plan, around Stalingrad-I station [S6], hatched areas | 17 with fill ≥ 0.80 | 67-121 × 34-93 m; median 96 × 59; most 94-117 × 49-74 | 212 × 88-97 m (two); 166-214 × 96-100 m at fill 0.75-0.79 |
| German plan, south of the station toward the Tsaritsa [S6] | 22 of 60 m or more | 64-120 × 33-93 m; median 98 × 57; most 94-110 × 47-74 | 152-212 × 60-116 m |
| 1908 plan, Zatsaritsyn part [S7] | 47 | 141-153 × 42-51 m | 141-174 × 72-99 m |
| 1908 plan, north of the centre | read by eye | strip blocks 130-170 × 30-40 m | |
| 1908 plan, old fortress quarter | read by eye | irregular blocks 25-40 × 80-120 m | |
| OSM street pitch, old grid south of the Tsaritsa [S8] | | 93-126 m across | 184-199 m along the Volga |

- Hatched areas on the 1942 plan exclude the block outline, so real blocks are about 2-4 m larger.
- The measurements agree:
  - central blocks are about 100 × 55-75 m (half blocks) or about 150-215 × 60-100 m (full blocks);
  - the street pitch is about 95-125 m by 185-235 m;
  - the fortress-era quarter by the Volga is irregular, with lanes at angles to the grid [D: S10n].

### 5.2 Streets [M/D]

- **Facade-to-facade gaps** between surviving pre-1917 houses on opposite sides of the same street [M: S8,
  S9]:
  - Kovrovskaya 13a-16a: 17.6 m;
  - Pugachyovskaya 14-15: 27.0 m;
  - Ogaryova 5-6: 28.9 m.
- **Ulitsa Lenina** (formerly Uspenskaya) beyond the avenue kept its "дореволюционные, довоенные размеры"
  [D: S10p]. It measures about 19-30 m between building lines: Lenina 15 (late 19th century) stands 10-12 m
  from the centreline and Lenina 17 (1906) 16-19 m, while the opposite line is a 1951 building 9-11 m from
  it [M: S8].
- **The 1942 plan** draws hatch-to-hatch gaps of 8.5-22 m, most commonly 12-17 m [M: S6]. The 1908 plan draws
  20-38 m [M: S7]. Both are map symbols; the facade measurements above govern.
- **Main streets** are about 18-22 m between building lines in the photographs; squares 60-150 m; broad
  fringe streets 30-40 m; timber-house streets unpaved [P L3].
- **The present Square of Fallen Fighters** is about 270 m long and 120-260 m wide. It is T-shaped and
  enlarged post-war [D: S14].
- **Surface.** Asphalt and electric street lamps first appeared in the centre in 1913 [D: S18]. Side streets
  were largely unpaved [P L3].
- **Summary for L [I].**

  | street class | width between building lines |
  |---|---|
  | main (tram) | 22-30 m |
  | ordinary | 17-22 m |
  | lane or passage | 8-14 m |
  | yard passage or gate | 3-4 m |
  | square | 60-250 m |

### 5.3 Yards and courtyards

- **Documented contents [D]:**
  - timber annexes with stairs and balconies [S10o2];
  - high single yard walls with timber stores against them [S10o];
  - brick gates [S10g];
  - summer kitchens (people lived in them in 1945, [S1]);
  - sheds [S9];
  - wooden warehouses and garages [S13];
  - annexes reaching deep into the yard behind the street range [S10r].
- **In March 1943** the yard buildings of the forshtadt are gone: the block interiors are open ground, and
  only the brick street ranges stand as roofless shells [D, visible in S28].
- **Inferred dimensions [I].** Full blocks 150-215 × 60-100 m give plots about 30-50 m deep, back to back.
  With street ranges 10-15 m deep, yards are 20-35 m deep and 15-35 m wide (the plot frontage). Half blocks
  give yards 12-25 m deep.

### 5.4 Trams, poles and wires

- **The network [D: S15 via Радциг 1962 and «Народный транспорт Царицына…» 2013; S16].**
  - Opened 1913; gauge 1524 mm.
  - Heavy "type 2A" rails on zinc-chloride-treated pine sleepers over sand ballast.
  - Steel tubular masts: over most of the route between the tracks with double brackets; in the city centre,
    wire hung on span wires between poles at the sides of the street.
  - Kh-series cars from 1929; eight routes in 1936; service stopped on 23 August 1942.
  - The network was "уничтожено полностью" (completely destroyed); the first rebuilt car ran in December
    1943.
- **Central tram streets.** Trams ran up Sovetskaya (formerly Astrakhanskaya) from the Astrakhansky bridge
  and turned onto Lenina [D: S10p]. 1914 photographs show trams on Knyagininskaya (Raboche-Krestyanskaya) and
  Gogolya [D: S16, S18].
- **Wrecked cars.** Werth passed "tramcars — many of them, all blasted, smashed and burned out" [D: S5
  p. 464]. Chuikov saw "изрешеченные пулями и осколками … трамваи" [D: S3 p. 393].
- **Poles and wires [P L4].**
  - Timber poles 7-9 m high with one to four crossarms, many leaning or broken off; wires sagging and hanging
    broken; lamp brackets.
  - In September 1942 streets were "забаррикадированы упавшими столбами, проводами, стенами" (blocked by
    fallen poles, wires, walls) [D: S3 p. 89; S4 pp. 75-76].
- **Inferred [I].**
  - Double track in the tram streets, track centres 3.3-3.8 m; single track in side streets.
  - Tram poles or masts every 30-40 m.
  - Contact wire about 5.5-6.0 m above the rail.
  - No Stalingrad-specific source was found for pole spacing or wire height.

### 5.5 Fences, walls, gates

- **Documented.** High brick yard walls [D: S10o]; brick gates with a yard entrance [D: S10g].
- **Photographs.** Picket fences 0.8-1.1 m and board fences 1.6-2.0 m in the timber quarters; brick-pier
  garden walls with pipe rails 0.6-1.0 m round squares [P L5]. A long brick street wall with pilasters,
  about 2.5-3 m high, appears in [S29: 08_knocked-out-t34-stalingrad-street_barch-183-b22359] [M, estimated
  against the T-34, ±20 %].
- **Inferred [I].** Yard walls 2.2-3.0 m high and 0.38-0.51 m thick (1.5-2 bricks); gate openings
  2.5-3.5 m wide.

---

## 6. The state by January 1943

### 6.1 Timeline in the centre [D]

| date | event |
|---|---|
| 23 August 1942 | Luftflotte 4 dropped about 1,000 t of bombs on the city [S26 citing Rees]. "Начались пожары, горели все деревянные здания" (fires broke out; all the wooden buildings burned) [S3 p. 341]. Trams stopped [S16]. |
| 13-27 September | The 13th Guards Division fought in the centre; the station changed hands repeatedly; the Square of Fallen Fighters was held from 15 to 27 September [S14]. |
| September to November | Front line through the centre: Pavlov's House, the mill and the Railwaymen's House. German storm groups took ground floors but not whole buildings [S11]. |
| By January 1943 | The Germans had turned the Square of Fallen Fighters into the main strongpoint of Paulus's southern group [S14]. |
| From 19-20 January | More than 16,000 wounded without care in the pocket, hunger, mass frostbite, no defensive lines [S27]. |
| 31 January | Paulus surrendered in the Univermag basement [S14, S5 pp. 464-466]. |

Over the whole battle more than 2.9 million bombs, shells and mortar rounds fell on the city [D: S2].

### 6.2 What stood

**Documented [D].**

- **Mid-September 1942.** "От деревянных домов остались только кучи золы, торчат печные трубы. Многие
  каменные дома — обгорелые, без окон и дверей, с провалившимися перекрытиями. Изредка попадаются уцелевшие
  здания." (Of the wooden houses only heaps of ash remained, with stove chimneys sticking up. Many stone
  houses were burned out, without windows or doors, their floors collapsed. Now and then an intact building.)
  [S3 p. 105]
- **Werth on the defence.** "In the area of the burned-down houses, everything was consumed by the flames …
  It was different in the area of the stone and brick houses; although nearly all these had been burned out,
  their walls and basements and steel skeletons afforded shelter and protection … once they had been burned
  out, there was little else that could burn" [S5 pp. 202-203]. Chuikov notes the advantage of burned-out
  stone buildings, "to which the enemy could not set fire before an attack" [S4 pp. 305-306].
- **Werth in February 1943.**
  - The large blocks of flats were "all burned out"; the shells of some were "almost complete — others
    presented fantastic zigzagged silhouettes, usually caused by heavy shelling".
  - Of the corner building opposite the Univermag "only the fragment of an enormous wall was rising in a
    sharp peak".
  - Two buildings in the square stood "squat and solid, though burned out": the Red Army House and the
    Univermag.
  - One intact wooden izba with lace curtains stood on the avenue, "an extraordinary freak".
  [S5 pp. 464-466]
- **The March 1943 air photograph** [S28]:
  - every roof in the forshtadt is gone;
  - the brick street ranges stand as open-topped shells with their cross walls;
  - the yard buildings are gone;
  - the streets remain clear lines.
- **Individual buildings:**

  | building | state after the battle |
  |---|---|
  | "Столичные номера", 4 storeys | timber floors and roof burned, all brickwork standing, no serious collapse of outer walls [S10i] |
  | Barrikadnaya 22 | roof, partitions and floors burned; central corner and an end collapsed; outer walls, stair shafts and part of the vaulted cellar ceilings survived [S10c] |
  | Profsoyuznaya 5a | roof lost, first-floor ceiling survived, "no large fire" [S10d] |
  | Kubanskaya 4 | nearly intact, original roof [S10e] |
  | Ogaryova 5 | slightly damaged [S10g] |
  | Zabolotny's house | "разрушен до основания" (razed) [S13]; "burned and blown up by the Germans", collapsing with its defenders [S11] |
  | the "milk house", the 4-storey office on 9 January Square | destroyed [S13] |
  | Univermag | top floor destroyed on 23 August, walls burned [S14, S3 p. 393] |

- **Official figures and the restoration that followed.**
  - Housing: 41,685 houses, 90.5 % of the pre-war stock, were destroyed [S17, S1]; S2 gives 41,895
    buildings.
  - The remaining stock was 9.5 % of pre-war, mostly 7,000 small one-storey buildings in the outlying
    Kirovsky district [S1].
  - Population on 2 February 1943: 32,181 people, 30,666 of them in the Kirovsky district; 1,515 in all the
    other districts; 33 in the Ermansky district [S17, S2].
  - The "surviving shells of ruined buildings" were restored first [S1].

**Inferred condition shares for the intro district [I].** Each building gets one dominant condition.

| typology | intact | burned | shelled | partly_collapsed |
|---|---|---|---|---|
| merchant_brick_house | 0.02 | 0.55 | 0.18 | 0.25 |
| apartment_block | 0.01 | 0.45 | 0.24 | 0.30 |
| wooden_house | 0.02 | 0.90 | 0.03 | 0.05 |
| workshop | 0.03 | 0.50 | 0.17 | 0.30 |

- **Razed sites.** A further 5-15 % of the pre-war masonry buildings should appear as razed plots: rubble
  mounds with no standing walls, placed as rubble dressing rather than as buildings. Documented cases are
  Zabolotny's house, the milk house and the office.
- **Reasoning.**
  - City-wide destruction was about 90 %, and the centre was almost uninhabited.
  - The March 1943 air photograph shows every roof gone and the masonry standing.
  - Werth describes "all burned out" blocks, with shell-shaped silhouettes and fragments where shelling was
    heavy.
  - Chuikov gives wooden houses as ash and chimneys.
  - Near-intact houses exist only at the edges of the fighting [S10e, S10g].
- **What each condition means for the model:**
  - `burned` is the default for masonry: roof and timber floors gone, walls to full height, frames gone,
    burned floor debris fallen into the cellar.
  - `shelled` means walls with breaches and some remaining concrete slabs or unburned floors.
  - `partly_collapsed` means a corner, an end or a section down [P G10-G12].
  - For wooden houses, `burned` leaves only the plinth, the stove and the chimney [D: S3 p. 105; P G13].

### 6.3 Rubble

**Documented [D].**

- "Повсюду — горы обломков рухнувших домов … Улицы и площади завалены битым кирпичом, искореженной
  арматурой, разбитой вражеской техникой, изрыты воронками" (everywhere mountains of debris from collapsed
  houses … streets and squares heaped with broken brick, twisted reinforcement, smashed enemy vehicles, dug up
  by craters) [S2].
- Werth: "With the accumulation of mountains of rubble, it became more difficult, and sometimes impossible,
  for the German tanks to move" [S5 pp. 205-206], and "piles of rubble, which no tank could penetrate"
  [S5 p. 211].

**Photographs [P G17].**

- A gutted building leaves a berm 1-2 m high along its base; its floors fall inside.
- Walls that fell outward make slopes of 1.5-3 m against 4-5-storey ruins.
- Mounds are 1-2 m.
- Flattened one-storey houses are mounds of 0.3-1.5 m.
- Rubble is loose brick with bonded lumps of 0.2-0.6 m, beams, rods, pipes and sheet.

**Mass-balance check [I].**

Assumptions:
- solid brickwork volume computed with window openings deducted;
- loose rubble = solid × 1.5 bulking [P G17, an engineering estimate].

Results:
- **4-storey block, 48 × 13 m, walls about 15 m high.**
  - Perimeter walls 0.58 m average, 22 % openings: about 830 m³.
  - Interior spine 0.38 m: about 220 m³.
  - Stair and section cross walls: about 170 m³.
  - Brick partitions: about 60 m³.
  - Total solid brickwork about 1,280 m³, giving about 1,920 m³ of loose rubble.
- **Its full collapse**, spread over the 624 m² footprint plus a 4 m apron (about 1,180 m²), averages 1.6 m
  deep, with a crest of 3-4 m.
- **One collapsed 12 m section** (25 %, about 480 m³ loose), spread over 12 × 19 m, averages 2.1 m, with a
  cone of 4-5 m against the stub wall.
- **A burned-only shell** loses only its floor pugging and partitions: about 300 m³, filling its cellar and
  leaving 0.3-1.0 m on the ground floor.
- **A collapsed one-storey merchant house** (20 × 11 m) averages about 0.7 m, with mounds of 1-1.5 m.
- **A two-storey one** (26 × 11 m) averages about 1.1 m, with crests of 2-2.5 m.

These agree with the photographic ranges.

**Clear width for L [I].** In an 18-22 m street between standing shells, berms of 2-4 m on each side leave a
clear lane of about 10-14 m [P L3]. Where a 4-5-storey facade fell outward, a 2-4 m ridge closes the street.
Such ridges are the historical tank barrier [D: S5].

### 6.4 Craters, wrecks, trees

- **Craters [D: S2].** More than 150,000 craters city-wide. Some reached groundwater ("В некоторых воронках
  … блестела вода"). Werth notes "enormous craters made by one-ton bombs" in the factory district [S5
  pp. 427-428].
  - Inferred density [I]: over a battle zone of about 80-200 km² (a strip about 40 km long, from the Mechetka
    to the Kuporosnaya balka, [S2]) the mean is 750-1,900 craters per km², about one per 500-1,300 m². The
    centre was denser. Most craters come from 50-250 kg bombs and 76-152 mm shells, about 1-10 m across; a
    few from 500-1,000 kg bombs are larger. These diameters are not sourced in this study.
- **Wrecks.** Knocked-out tanks stand in streets and squares; vehicles are abandoned along one side of a road
  [P L11]. On 4 February 1943 the streets were "запруженные разбитой военной техникой врага" (jammed with the
  enemy's wrecked vehicles) [D: S3 p. 393].
- **Trees.** Stripped trunks 10-15 m tall [P L6]. In September "ни одной зеленой ветки … все погибло в огне"
  (not one green branch; everything perished in the fires) [D: S3 p. 105]. Werth saw an avenue with
  "half-shattered trees on either side" [D: S5 p. 464].

### 6.5 Snow and weather

- **Monthly mean air temperature, Volgograd (station 34560)** [D: S24]:

  | month | mean |
  |---|---|
  | December 1942 | -10.3 °C |
  | January 1943 | -8.4 °C |
  | February 1943 | -2.1 °C |
  | January 1942, for comparison | -12.5 °C |

- **Climate.** Tsaritsyn's mean winter temperature is -8.6 °C, with 280 mm of precipitation a year: a dry
  steppe with shallow snow [D: S23]. The German Mil-Geo sheet gives a midwinter mean of -9.9 °C [D: S6,
  catalogue description].
- **Snow depth [P L8].**
  - 5-15 cm on open ground, trodden to a crust, with brick edges showing;
  - 20-30 cm on squares;
  - 0.3-0.8 m drifted inside roofless buildings;
  - on every ledge and wall top.

  This study found no measured snow depth for January 1943.

---

## 7. Defensive changes

### 7.1 Strongpoints and fire in tiers [D]

- **Strongpoints.** "В качестве опорных пунктов служили городские постройки, особо прочные каменные и
  кирпичные здания. Приспособленные к обороне, они связывались с другими домами траншеями и ходами
  сообщений" (the strongpoints were city buildings, especially solid stone and brick ones; adapted for
  defence, they were linked to other houses by trenches and communication trenches) [S3 p. 296].
  - Garrisons included anti-tank rifles, anti-tank guns, sappers, snipers "и обязательно медицинского
    работника" (and always a medical orderly) [S3 pp. 296-297].
- **Fire in tiers.** "В многоэтажных зданиях организовывалась многоярусная оборона: в полуподвальных и нижних
  этажах … для ведения огня вдоль улиц, в верхних этажах и на чердаках — для ведения огня сверху по танкам,
  по улицам, дворам, соседним зданиям" (in multi-storey buildings the defence was organised in tiers:
  semi-basements and lower floors fired along the streets; upper floors and attics fired down on tanks,
  streets, yards and neighbouring buildings) [S3 pp. 297-298].
  - Direct-fire guns and some heavy machine guns stood on the lower floors; heavy machine guns for long range
    sometimes stood on the upper floors [S3 p. 297].
- **Ambush openings.** Anti-tank fire came "преимущественно из засад, для чего использовались проломы в
  заборах, стенах, а также подъезды домов, ворота, окна" (mainly from ambushes, using breaches in fences and
  walls, doorways, gates and windows) [S3 p. 298].
- **Obstacles.** Streets and squares were "перегораживались различными препятствиями" (blocked with various
  obstacles), and the approaches were covered by flanking fire from neighbouring buildings [S3 p. 296].
- **The rule for the ground.** Where the defence was stone buildings with cellars, "Деревянные,
  одноэтажные здания, здания с глинобитными стенами игнорировались и служили нейтральной полосой" (wooden,
  one-storey and mud-walled buildings were ignored and served as no man's land) [S13].
- **Where the fighting was.** "Городской бой … это бой за дома … в комнатах, на чердаках, на крышах, в
  подвалах, в развалинах и реже всего на улицах и площадях" (city fighting is a battle for houses … in rooms,
  attics, roofs, cellars and ruins, and least of all in streets and squares) [S3 p. 309].

### 7.2 Loopholes, blocked windows and entrances [D]

- **Pavlov's House.** "Для стрельбы были проделаны узкие амбразуры в окнах и в стенах" (narrow embrasures
  were made in the windows and walls for firing) [S11]. Walls in the basement and upper floors were broken
  through to move inside [S12; S26 citing Beevor 1998 p. 198].
- **Lieutenant Dragan's corner house** (3 storeys, Krasnopiterskaya and Komsomolskaya) [S3 p. 157;
  S4 p. 157]:
  - "забаррикадировать все выходы, приспособить окна и проломы под амбразуры" (all exits barricaded, windows
    and breaches adapted as embrasures);
  - a heavy machine gun "у полуподвального окна" (at a semi-basement window);
  - groups on the third floor and in the attic broke up a brick pier ("разобрать кирпичный простенок") to
    drop stones and beams on attackers;
  - the cellar held the badly wounded.
- **The Germans in the Railwaymen's House** "had bricked up the entrances to the building, leaving
  passageways to the firing points only through the basement". A wall through the basement had been broken
  down across the whole width, and a blank wall divided the house [S4 p. 320].
- **The Univermag basement.** Paulus's window, "just above the level of the pavement outside", had been
  sandbagged [S5 pp. 465-466].
- **Loophole sizes are not documented** in the sources read. Slits of 0.15-0.25 m appear only in a Berlin
  1945 photograph [P G18]. Inferred [I]: window embrasures made by stacking brick or sandbags in the lower
  60-80 % of a window, leaving a 0.2-0.4 m slit; wall loopholes 0.15-0.3 m knocked through the piers.

### 7.3 Barricades, obstacles, mines, wire [D]

- **The first barricades were weak.** At the end of August 1942 Chuikov found the civilian street barricades
  useless: "Not only will they not hold back enemy tanks — the bumper of a lorry will knock them down"
  [S4 p. 87]. The 10th NKVD Division's blockhouses were "25-30 per cent … completed" [S4 p. 102].
- **After the bombing**, streets were blocked by fallen poles, wires, bricks and walls [S3 p. 89;
  S4 pp. 75-76].
- **A strong barricade.** In September a Soviet correspondent at the Square of Fallen Fighters saw "a
  barricade across a street, large, strong and reliable, and a little beyond that is the front line"
  [S5 pp. 230-231].
- **Hedgehogs** of crossed rails and I-beams appear in [S29: 04_red-army-men-in-ruined-workshop_1942-11]
  [P L10].
- **Pavlov's House** was surrounded by wire. Its approaches from the German side were mined [S11]; the
  English sources say four belts of wire and mines on all approaches to the square [S12]. Chuikov lists
  "заграждений с минами" (obstacles with mines) in the anti-tank defence [S3 pp. 298-299].
- **Dug-in tanks** stood as firing points: the 84th Brigade on Tramvaynaya Street [S3 ch. 8; S4 p. 202].
- **The January 1943 German defence** in the city: dzots and dugouts; a transformer kiosk "превращенной
  противником в дот" (turned by the enemy into a pillbox) [S3 p. 371]. The Germans "crawled into deep holes
  and stone cellars … into concrete water tanks, into wells and sewage pipes, into underground tunnels"
  [S5 pp. 427-428].

### 7.4 Communication trenches and dugouts [D]

- **Pavlov's House** had "глубокий ход сообщения" (a deep communication trench) to the regiment's positions
  for supply, and a fully buried telephone line [S11].
- **Assault rules.** Storm groups dug trenches to close with the enemy at night ("сближайся с противником
  траншеями … рой ночью траншеи"), and on taking a building "немедленно сооружать ходы сообщения"
  (immediately build communication trenches) [S3 pp. 307-309]. Sappers blew communication trenches with
  explosives [S4 pp. 344-345].
- **Trenches through yards and factories.** At Red October trenches ran "through the factory yards; through
  the workshops themselves" [S5 p. 453].
- **Dugouts.** A command dugout was "a wide slit" roofed with brushwood and straw under 10-20 cm of earth,
  with earthen bench, bed and table [S3 p. 106]. People sheltered in slit trenches (щели), pits and craters
  [S3 p. 105].
- **Squares** carried trenches with spoil ridges of 0.5-1 m [P L10; S29: 04_univermag-paulus-hq-square_1943].

### 7.5 Cellars as shelters, aid posts and headquarters [D]

- **Soviet aid posts.** Battalion aid posts were set up "в различных укрытиях (блиндажах, землянках,
  подвалах домов и т. д.)" (in dugouts, earth shelters, house cellars) [S3 p. 350].
- **Civilians.** Civilians sheltered in the Pavlov's House cellars throughout the defence, from
  23 September to 25 November 1942 [S11; S12].
- **German headquarters and wounded.**
  - Paulus's headquarters was in the Univermag basement; its corridor was lined with small cubicle offices
    [S5 p. 466; S14].
  - Two hundred dying Germans lay in the basement of the burned-out Red Army House on 3-5 February 1943
    [S5 pp. 466-467].
  - From 19-20 January more than 16,000 wounded lay without care in the pocket [S27].
- **After the battle.** People lived "в блиндажах, землянках, на лестничных площадках домов, в подвалах" (in
  dugouts, earth shelters, on the stair landings of houses, in cellars) [S2]. In 1945, 40,000 people still
  lived in dugouts, cellars, summer kitchens and stair landings [S1].
- **Chuyanov, 3 February 1943.** "Из подвалов несло едким дымом и смрадом тлеющих трупов" (acrid smoke and
  the stench of smouldering bodies came from the cellars) [S2]. For the story (S), this is restrained,
  documented evidence; it is not for display beyond what R2 sources.

---

## 8. Facts that constrain a tank in those streets

### 8.1 The vehicle [D]

- **Tiger I** [S25]:
  - 6.316 m hull length, 8.45 m with the gun forward;
  - 3.56 m wide on English Wikipedia (citing Jentz & Doyle), 3.70 m on German Wikipedia;
  - 3.00 m high; ground clearance 0.47 m;
  - climbs slopes up to 35°; crosses trenches of 2.50 m; wades 1.60 m;
  - turret roof and hull floor armour 25 mm.
- **KwK 36 gun elevation:** -8° to +15° [S25].
- **Game rig** [S31]:
  - gun trunnion 2.37 m above the ground (0.524 m turret pivot + 1.845 m);
  - trunnion 1.04 m forward of the turret axis, gun length 4.79 m, so the muzzle is about 5.83 m from the
    turret axis;
  - collision footprint 4.12 × 7.13 m.
- **T-34** for comparison: 3.00 m wide, 2.46 m high [S25].
- **History note.** No Tigers fought inside the pocket (brief §2, and S31 via
  docs/teams/historical-map-grounding.md). German armour in the city was Panzer III/IV and assault guns; not
  researched here.

### 8.2 Widths [I from D/M]

- **The lane.** With rubble berms of 2-4 m on each side, an 18-22 m street leaves a 10-14 m lane (§6.3). The
  game's 4.12 m collision box needs a clear lane of at least about 4.6-5 m. Lanes and yard passages of
  8-14 m admit it; gates of 2.5-4 m do not.
- **Turret traverse** needs a clear radius of about 5.9 m from the turret axis to swing the barrel across the
  street: about 11.7 m of clear width if the tank is centred. In 10-14 m lanes between rubble berms the
  barrel strikes standing walls or rubble when turned to 90°, so the hull must be turned. The same holds in
  lanes of 8-14 m.

### 8.3 Firing positions above: the elevation limit [I, computed from D]

The highest point the gun can reach at horizontal distance d from the trunnion is
h = 2.37 + d · tan 15° = 2.37 + 0.268 d:

| d | reach |
|---|---|
| 10 m | 5.0 m |
| 15 m | 6.4 m |
| 20 m | 7.7 m |
| 30 m | 10.4 m |
| 45 m | 14.4 m |

A 1930s block has its ground floor at +1.0 m and storey pitch 3.3 m [P G1], so its storey floors are at 1.0,
4.3, 7.6 and 10.9 m and its parapet at about 15 m. From that:

- the third-storey sill (about 8.5 m) needs d ≥ 23 m;
- the fourth-storey sill (about 11.8 m) needs d ≥ 35 m;
- the attic and parapet need d ≥ 47 m.

A tank inside a 20-24 m street is 16-21 m from the opposite facade. It reaches only the ground and first
floors across the street, and nothing on its own side. This matches the documented tiered defence, with fire
on tanks from upper floors and attics [D: S3 pp. 297-298]. Pavlov's defenders moved up "when German Panzers
approached so they could fire antitank rifles down onto the tanks' vulnerable, thin roofs" [D: S32]; an
anti-tank rifle stood on the roof [D: S12].

The Germans used tank fire at a distance against upper floors: "противник бросил в бой 10 танков и прямой
наводкой вывел из строя верхние этажи «П»-образного здания" (the enemy sent in 10 tanks and knocked out the
upper floors of the U-shaped building with direct fire) (138th Division war diary, 11 November 1942) [D:
S10k2].

### 8.4 Firing positions below: the depression limit [I, computed from D]

The lowest point the gun can reach is h = 2.37 - d · tan 8° = 2.37 - 0.1405 d:

- ground level at the facade needs d ≥ 16.9 m;
- a semi-basement window sill at 0.3 m needs d ≥ 14.7 m;
- at 10 m the gun cannot hit below 0.97 m; at 5 m, not below 1.67 m.

Semi-basement embrasures firing along the street [D: S3 p. 297; S3 p. 157] are therefore out of the main
gun's reach whenever the tank is within about 15 m, unless the hull is pitched nose-down on rubble.

### 8.5 Rubble and ground [D/P/I]

- **Documented.** Rubble made German tank movement "more difficult, and sometimes impossible" [D: S5
  pp. 205-206]; there were "piles of rubble, which no tank could penetrate" [D: S5 p. 211].
- **Slopes.** Berms of 1-2 m and slopes of 1.5-3 m [P G17]. Loose brick rubble rests at about 35-40° [I,
  engineering assumption]: at or beyond the Tiger's documented 35° climbing limit. Crests above about 0.5 m
  can belly a 0.47 m-clearance hull [I].
- **Treat as barriers** [I]: ridges of 3-4 m from fallen 4-5-storey facades, and the rubble at collapsed
  sections (cones of 4-5 m).
- **Cellars under footprints [I].** Burned buildings have no ground floor over their cellars: the timber
  floor burned and fell in, leaving debris-filled pits 2-2.5 m deep. A vehicle entering a footprint
  physically meets a void or loose fill, not level ground.
- **Craters [D].** More than 150,000 city-wide, some reaching groundwater [S2]; they break up open ground and
  squares (§6.4).

---

## 9. Uncertainties and conflicts

1. **Merchant storey height.**
   - Photo analysis: 3.8-4.6 m [P G1].
   - My single-photo reading: 3.2-3.7 m (±15 %).
   - Proposed: 4.0 m typical, range 3.4-4.6 m. No drawing with a height was found.
2. **Street widths.**
   - The maps disagree: 1908 draws 20-38 m, 1942 draws 9-22 m.
   - The facade-to-facade measurements (17.6-29 m) and the photographs (18-22 m) are used.
   - Some modern street lines moved after the war: Sovetskaya was widened on its Volga side and Kostromskaya
     shifted 10-20 m [D: S10p, S10s]. Measurements were taken only between surviving pre-1917 houses.
3. **The sample of surviving houses** is biased towards the less-fought Voroshilovsky district and to robust
   buildings. Present-day storeys and annexes inflate some footprints: Ostrovskogo 3 gained a storey in the
   1960s, Ogaryova 6 was enlarged in 1946 [D: S10j, S10f].
4. **Condition shares, block recipes and typology shares are inferred.** No count of the centre's buildings
   by type or condition was found.
5. **Pole spacing, contact-wire height, loophole sizes, crater diameters and snow depth** have no
   Stalingrad-specific source. The values given are inferred or taken from the photographs.
6. **Destroyed buildings.** Kuznetsova and Wikipedia give 41,685 houses [S1, S17]; the city history site
   gives 41,895 buildings [S2]. Remaining civilians: the city census gives 32,181 on 2 February 1943 [S2];
   Beevor gives 9,796 [S26 citing Beevor 1998 p. 407]. Different dates and definitions.
7. **Tiger width** is 3.56 m or 3.70 m in the two articles [S25]. The game's collision box (4.12 m) governs
   passability.
8. **The kit's 4 m bay** is wider than the real window axes of 1930s blocks (2.7-3.4 m) and merchant houses
   (2.6-3.4 m). See G's decision on sub-bays [P G2].

---

## 10. Recommendations to G and L (short)

**For G:**

- Widen `merchant_brick_house` storeys to 1-3, allowing 4 only for rare raised houses.
- Set `apartment_block` storeys to 3-6.
- Keep wing depths near the measured medians: about 11 m merchant, about 13-14 m block.
- Give every floor its construction, timber or concrete, so that `burned` is realistic [P G9].
- Put semi-basement windows on merchant houses and blocks. They are the documented street-level firing
  positions.

**For L:**

- Block pitch about 100-125 × 185-235 m.
- Ordinary streets 17-22 m; one 22-30 m tram street with span-wire poles; a square of 60-250 m.
- Yards behind continuous brick street walls.
- A fringe of burned timber plots with standing stove stacks.
- Rubble berms at the base of shells, and at least one street closed by a fallen facade.

---

## 11. References

Online sources were accessed on 28 September 2026. Page numbers for S3 are the printed pages marked in the
online text. S4 and S5 page numbers were read from the scans' running heads (±1 page).

**Primary and statistical sources**

- **[S1]** Кузнецова Н. В. "Восстановление жилого фонда Сталинграда в 1943-1953 годах". *Вестник
  Волгоградского гос. университета, Серия 4*, 2003, вып. 8, с. 12-19. Cites ЦДНИВО f. 113; Чуянов 1976,
  с. 263; Водолагин 1968, с. 353.
  https://cyberleninka.ru/article/n/vosstanovlenie-zhilogo-fonda-stalingrada-v-1943-1953-godah
- **[S2]** "Возрождение". История Сталинграда, volgograd-history.ru. Includes Chuyanov's notes of
  3 February 1943 and the census of 2 February 1943.
  http://www.volgograd-history.ru/history/stalingrad/article_008.html
- **[S3]** Чуйков В. И. *Сражение века*. М.: Советская Россия, 1975. Online text, chapters 04-11:
  http://militera.lib.ru/memo/russian/chuykov/index.html
- **[S4]** Chuikov V. I. *The Battle for Stalingrad*. Trans. H. C. Stevens. New York: Holt, Rinehart and
  Winston, 1964. Full text: https://archive.org/details/vasily-chuikov-book
- **[S5]** Werth A. *The Year of Stalingrad*. London: Hamish Hamilton, 1946. Full text (Digital Library of
  India): https://archive.org/details/in.ernet.dli.2015.209933
- **[S6]** Generalstab des Heeres, Abt. für Kriegskarten und Vermessungswesen. *Stadtplan von Stalingrad*,
  1:20,000, "angefertigt nach einer Bildplanskizze im Juni 1942", Stand 1.8.1942. David Rumsey Map
  Collection 10842.001 (CC BY-NC-SA 3.0). https://archive.org/details/dr_stadtplan-von-stalingrad-10842001
  The verso Mil-Geo-Karte is 10842.002: https://archive.org/details/dr_mil-geo-karte-of-stalingrad-10842002
- **[S7]** *План города Царицына с адресным указателем*, масштаб 100 саженей в дюйме. In: *Спутник по реке
  Волге…*, вып. VIII. Саратов, 1908. https://commons.wikimedia.org/wiki/File:Tsaritsyn_plan_1908.jpg
- **[S8]** OpenStreetMap contributors (ODbL), via https://api.openstreetmap.org and
  https://overpass-api.de (data as of May-September 2026). Relations and ways cited:
  - relation 20144900 (Pavlov's House);
  - way 123802770 (Gerhardt mill);
  - way 124698047 (Univermag).
- **[S9]** Список объектов культурного наследия регионального значения Волгограда (Wikipedia list with
  coordinates).
  https://ru.wikipedia.org/wiki/Список_объектов_культурного_наследия_регионального_значения_Волгограда
- **[S24]** Средние месячные температуры воздуха, Волгоград (34560). pogodaiklimat.ru.
  http://www.pogodaiklimat.ru/history/34560.htm
- **[S28]** German reconnaissance air photographs of Stalingrad, August 1942 and March 1943; fragments
  published in [S10n]: http://xn--80aqpk2ad9a.xn--p1ai/wp-content/uploads/2023/12/002.jpg and .../003.jpg

**Царицын.рф building histories — [S10]**

Local architectural history site, ed. R. Shkoda: http://xn--80aqpk2ad9a.xn--p1ai/

| key | article | date | URL |
|---|---|---|---|
| S10a | "Деревянная застройка ул. Пугачёвской" | 2026 | http://xn--80aqpk2ad9a.xn--p1ai/2026/03/3833-pugach_11-12.html |
| S10b | "Царицынские кирпичи" | 2024 | http://xn--80aqpk2ad9a.xn--p1ai/2024/10/3401-kirpichi.html |
| S10c | "Баррикадная, 22" | 2026 | http://xn--80aqpk2ad9a.xn--p1ai/2026/01/3816-barr22.html |
| S10d | "Профсоюзная, 5а" | 2026 | http://xn--80aqpk2ad9a.xn--p1ai/2026/05/3850-prof5a.html |
| S10e | "Кубанская, 4" | 2025 | http://xn--80aqpk2ad9a.xn--p1ai/2025/12/3716-kuban4.html |
| S10f | "Огарёва, 6" | 2025 | http://xn--80aqpk2ad9a.xn--p1ai/2025/06/3590-ogarova6.html |
| S10g | "Огарёва, 5" | 2026 | http://xn--80aqpk2ad9a.xn--p1ai/2026/01/3753-ogariova5.html |
| S10h | "Мавританские мотивы купца Рысина" | 2024 | http://xn--80aqpk2ad9a.xn--p1ai/2024/06/3248-rysin-2.html |
| S10i | "Эволюция площади Павших борцов. Часть 2" | 2015 | http://xn--80aqpk2ad9a.xn--p1ai/2015/10/504-square2.html |
| S10j | "Сандрики дома Сусоколова" | 2024 | http://xn--80aqpk2ad9a.xn--p1ai/2024/05/3041-susokolov-2.html |
| S10k | "Дом НКВД" | 2018 | http://xn--80aqpk2ad9a.xn--p1ai/2018/04/790-dom-nkvd.html |
| S10k2 | "Дом комиссаров" | 2019 | http://xn--80aqpk2ad9a.xn--p1ai/2019/09/853-dom-komissarov.html |
| S10l | "Мельница Гергардта" | 2015 | http://xn--80aqpk2ad9a.xn--p1ai/2015/03/533-melnica-gergardta.html |
| S10m | "Дом Лапшина" | 2022 | http://xn--80aqpk2ad9a.xn--p1ai/2022/09/397-dom-lapshina.html |
| S10n | "Дом на Мариинской" | 2023 | http://xn--80aqpk2ad9a.xn--p1ai/2023/12/2572-mariinskaya-2.html |
| S10o | "Дом с Екатерининской" | 2023 | http://xn--80aqpk2ad9a.xn--p1ai/2023/12/2664-ekateriniskaya.html |
| S10o2 | "Комсомольская, 10а" | 2023 | http://xn--80aqpk2ad9a.xn--p1ai/2023/12/2697-koms10a.html |
| S10p | "Перекрёсток Ленина и Советской" (photos from П. П. Олейников, *Архитектурное наследие Сталинграда*) | 2013 | http://xn--80aqpk2ad9a.xn--p1ai/2013/05/155-perekrestok.html |
| S10q | "Дом Яблокова" | 2014 | http://xn--80aqpk2ad9a.xn--p1ai/2014/02/317-yablokov.html |
| S10r | "Дом Мишнина" | 2022 | http://xn--80aqpk2ad9a.xn--p1ai/2022/11/847-dom-mishnina.html |
| S10s | "Орловская, 20" | 2026 | http://xn--80aqpk2ad9a.xn--p1ai/2026/01/3766-orlovskaya.html |

**Wikipedia and Wikisource articles on buildings and places**

- **[S11]** "Дом Павлова", Russian Wikipedia. Cites Гуркин В. 1963 and the memoir of W. Adam.
  https://ru.wikipedia.org/wiki/Дом_Павлова
- **[S12]** "Pavlov's House", English Wikipedia. https://en.wikipedia.org/wiki/Pavlov%27s_House
- **[S13]** "Мельница Гергардта", Russian Wikipedia. https://ru.wikipedia.org/wiki/Мельница_Гергардта
- **[S14]** "Площадь Павших Борцов", Russian Wikipedia. Cites Олейников П. П. et al., 2011.
  https://ru.wikipedia.org/wiki/Площадь_Павших_Борцов
- **[S15]** "Трамвай № 1 (Волгоград)", Russian Wikipedia. Cites Радциг В. 1962 and *Народный транспорт
  Царицына — Сталинграда — Волгограда*, 2013. https://ru.wikipedia.org/wiki/Трамвай_№_1_(Волгоград)
- **[S16]** "Волгоградский трамвай", Russian Wikipedia. https://ru.wikipedia.org/wiki/Волгоградский_трамвай
- **[S17]** "Волгоград", Russian Wikipedia, history, losses and architecture sections.
  https://ru.wikipedia.org/wiki/Волгоград
- **[S18]** "История Волгограда", Russian Wikipedia. https://ru.wikipedia.org/wiki/История_Волгограда
- **[S19]** "Центральный район (Волгоград)", Russian Wikipedia.
  https://ru.wikipedia.org/wiki/Центральный_район_(Волгоград)
- **[S22]** "Особняк Репниковой", Russian Wikipedia. https://ru.wikipedia.org/wiki/Особняк_Репниковой
- **[S23]** "Царицын", Энциклопедический словарь Брокгауза и Ефрона (Wikisource).
  https://ru.wikisource.org/wiki/ЭСБЕ/Царицын
- **[S27]** "Операция «Кольцо» (1943)", Russian Wikipedia.
  https://ru.wikipedia.org/wiki/Операция_«Кольцо»_(1943)

**Construction references**

- **[S20]** "Сталинка", Russian Wikipedia. https://ru.wikipedia.org/wiki/Сталинка
- **[S21]** Russian Wikipedia:
  - "Кирпичная кладка", citing *Каменные работы*, 1987, §2: https://ru.wikipedia.org/wiki/Кирпичная_кладка
  - "Кирпич", on the 1927 standard: https://ru.wikipedia.org/wiki/Кирпич
  - [S21b] "Русская печь", citing *Русская изба*, 2004: https://ru.wikipedia.org/wiki/Русская_печь
  - [S21c] "Полукаменный дом": https://ru.wikipedia.org/wiki/Полукаменный_дом

**Vehicles and battle history**

- **[S25]** Vehicle articles:
  - "Tiger I", English Wikipedia (citing Jentz & Doyle): https://en.wikipedia.org/wiki/Tiger_I
  - "Panzerkampfwagen VI Tiger", German Wikipedia: https://de.wikipedia.org/wiki/Panzerkampfwagen_VI_Tiger
  - "8.8 cm KwK 36": https://en.wikipedia.org/wiki/8.8_cm_KwK_36
  - "T-34": https://en.wikipedia.org/wiki/T-34
- **[S26]** "Battle of Stalingrad", English Wikipedia. Cites Beevor 1998 pp. 198 and 407; Rees; Hellbeck 2015
  p. 251; Glantz & House 2009. https://en.wikipedia.org/wiki/Battle_of_Stalingrad
- **[S32]** "Defending the City: An Overview of Defensive Tactics from the Modern History of Urban Warfare",
  Modern War Institute at West Point (secondary; on Pavlov's House).
  https://mwi.westpoint.edu/defending-the-city-an-overview-of-defensive-tactics-from-the-modern-history-of-urban-warfare/

**Project material**

- **[S29]** Reference photo set, `/home/alex/panzer/reference/wartime_photos_20260928/` (manifest.json;
  archive IDs per file).
- **[S30]** `PHOTO_REFERENCE_ANALYSIS_20260928.md` (P), rules G1-G20 and L1-L14, same research folder.
- **[S31]** Project files:
  - `docs/URBAN_INTRO_TEAM_20260928.md` and `docs/teams/historical-map-grounding.md`;
  - `src/tank.rs` (`TIGER_PARTS`);
  - `src/anti_infantry_ai.rs` (`GUN_MIN_PITCH` -0.13963, `GUN_MAX_PITCH` 0.26180);
  - `src/building_grammar.rs` (provisional limits).

---

## 12. Parameter table

### urban_fabric_parameters

```json
{
  "urban_fabric_parameters": {
    "schema": "r1-urban-fabric/1",
    "date": "2026-09-28",
    "scope": "Stalingrad city centre (Ermansky district and adjoining old town), state of January 1943",
    "units": "metres unless stated; shares are fractions of 1; angles in degrees",
    "provenance_codes": {
      "D": "documented by the cited source",
      "M": "measured by R1 from a cited map, dataset or photograph",
      "P": "photo-analysis cross-check (PHOTO_REFERENCE_ANALYSIS_20260928.md)",
      "I": "inferred by R1; a proposal for the game, not a historical fact"
    },
    "kit_note": "Real window axes (2.6-3.4 m) are narrower than the contract's 4 m kit bay; footprint ranges below are real sizes, and recommended_grammar_limits_4m_bay quantises them.",
    "typologies": {
      "merchant_brick_house": {
        "frontage_m": {"range": [9.5, 63.1], "typical": [18.3, 34.3], "median": 26.6, "prov": "M", "src": ["S8", "S9"], "note": "long side of 36 surviving pre-1917 masonry buildings"},
        "depth_m": {"wing_range": [8.4, 16.5], "wing_typical": [9.8, 12.0], "wing_median": 11.1, "bounding_depth_with_yard_wings": [11.3, 33.5], "prov": "M", "src": ["S8", "S9"]},
        "storeys": {"range": [1, 4], "typical": [1, 2], "note": "3-4 only where raised in the 1920s-30s", "prov": "D", "src": ["S10i", "S10m", "S10q", "S8"]},
        "storey_height_m": {"typical": 4.0, "range": [3.4, 4.6], "ground_floor_with_shop": [4.0, 4.8], "plinth_or_semibasement_above_grade": [0.6, 1.2], "cornice_of_two_storey_house": [6.5, 10.0], "prov": "P+M+I", "src": ["S30:G1", "S29"]},
        "bay_m": {"typical": 2.9, "range": [2.6, 3.4], "prov": "M+P", "src": ["S10j", "S8", "S30:G2"]},
        "wall_thickness_m": {"exterior_ground": [0.51, 0.68], "exterior_upper": [0.51, 0.55], "semi_basement": [0.64, 0.77], "interior_bearing": [0.38, 0.51], "partition": [0.10, 0.15], "prov": "I", "src": ["S21", "S30:G3"], "note": "brick-count conversion 2 = 0.51, 2.5 = 0.64 m documented; counts inferred; pre-1927 bricks up to ~7 % longer"},
        "brickwork": {"bond_before_1929": "Flemish", "bond_after_1929": "English cross", "strong_buildings": "header bond", "finish": "red face brick, often whitewashed; plaster keystones and details", "prov": "D", "src": ["S10b", "S10f", "S10j"]},
        "roof": {"type": "hipped or gabled sheet iron on timber rafters, behind a parapet or attics on the street facade", "pitch_deg": [15, 25], "prov": "D+P+I", "src": ["S10e", "S10f", "S10d", "S30:G4"]},
        "cellar": {"share": 0.6, "kind": "semi-basement (tsokol) or vaulted cellar", "floor_below_grade_m": [1.0, 1.8], "headroom_m": [2.2, 2.8], "prov": "D examples, I share and depth", "src": ["S10e", "S10h", "S22", "S10q", "S30:G6"]},
        "floor_construction": {"upper": "timber joists", "over_cellar": "brick vaults or timber", "share_timber_floors": 0.95, "prov": "D examples, I share", "src": ["S10l", "S10i"]},
        "windows_m": {"upper": {"w": [1.0, 1.4], "h": [1.8, 2.2]}, "shop": {"w": [2.0, 2.5], "h": [2.0, 2.5]}, "semi_basement": {"w": [0.8, 1.2], "h": [0.5, 0.8], "sill_above_grade": [0.2, 0.4]}, "sill_above_floor": 0.9, "heads": "segmental or round arches with carved brick hoods", "shutters": "ground floor shuttered", "prov": "P+M+D+I", "src": ["S30:G2", "S29", "S10f"]},
        "doors_m": {"entrance": {"w": [1.1, 1.4], "h": [2.1, 2.6]}, "shop": {"w": [1.4, 2.0], "h": [2.4, 3.0]}, "yard_gate": {"w": [2.5, 3.5], "h": [3.0, 4.0]}, "prov": "P+I", "src": ["S30:G2"]},
        "stairs": {"kinds": ["internal timber stair", "stair in a yard risalit", "external timber gallery with stair (burned by 1943)", "spiral metal stair on the yard side"], "prov": "D", "src": ["S10f", "S10o2", "S22"]}
      },
      "apartment_block": {
        "frontage_m": {"range": [27.0, 80.0], "typical": [40.0, 60.0], "section_length_m": [12.0, 16.0], "prov": "M+I", "src": ["S8", "S11"], "note": "Pavlov's House 4 entrances in ~56 m"},
        "depth_m": {"range": [11.5, 16.0], "median": 13.7, "prov": "M", "src": ["S8"]},
        "storeys": {"range": [3, 6], "typical": [4, 5], "prov": "D", "src": ["S11", "S14", "S10c", "S5"]},
        "storey_height_m": {"typical": 3.3, "range": [3.0, 3.5], "ground_floor_raised_above_street": [0.8, 1.2], "ceiling_min": 3.0, "prov": "P+D", "src": ["S30:G1", "S20"]},
        "bay_m": {"typical": 3.0, "range": [2.7, 3.4], "prov": "P+M", "src": ["S30:G2", "S29"]},
        "wall_thickness_m": {"exterior": [0.51, 0.64], "exterior_ground": 0.64, "interior_bearing": [0.25, 0.38], "partition": [0.08, 0.12], "prov": "D (general 1930s-50s practice) + I", "src": ["S20", "S21"]},
        "roof": {"type": "hipped or gabled sheet iron on timber (e.g. Pavlov's House), or flat behind a brick parapet 0.8-1.0 m (constructivist)", "pitch_deg": [0, 25], "prov": "D+P", "src": ["S8", "S20", "S30:G4"]},
        "cellar": {"share": 0.85, "floor_below_grade_m": [1.8, 2.4], "headroom_m": [2.0, 2.5], "prov": "D examples, I share", "src": ["S11", "S10c", "S14", "S20", "S30:G6"]},
        "floor_construction": {"share_timber_joists": 0.65, "share_reinforced_concrete": 0.35, "stairs": "concrete flights and landings", "cellar_ceiling": "brick vaults or concrete", "prov": "D examples, I shares", "src": ["S10c", "S11", "S30:G9"]},
        "windows_m": {"flat": {"w": [1.3, 1.45], "h": [1.45, 1.6]}, "tall_grouped_variant": {"w": [0.9, 1.0], "h": [2.1, 2.3]}, "stair": "tall vertical strip per landing", "balcony_door_h": 2.4, "sill_above_floor": 0.9, "prov": "P+M", "src": ["S30:G2", "S29"]},
        "doors_m": {"stair_entrance": {"w": [1.2, 1.5], "h": [2.2, 2.5]}, "flat": {"w": 0.9, "h": 2.1}, "shop_or_public_entrance_w_max": 2.5, "prov": "P+I", "src": ["S30:G2"]},
        "stairs": {"per_section": 1, "flights_per_storey": 2, "flight_width": [1.05, 1.2], "stair_bay": {"w": [2.6, 3.0], "d": [5.0, 6.0]}, "flats_per_landing": [2, 4], "prov": "D flats per landing; I dimensions", "src": ["S20"]}
      },
      "wooden_house": {
        "frontage_m": {"range": [6.0, 14.0], "typical": [8.0, 12.0], "plot_frontage": [8.0, 12.0], "prov": "P+I", "src": ["S30:L2"], "note": "the documented survivors (railway staff houses) are 22-24 m long"},
        "depth_m": {"range": [6.0, 12.0], "typical": 8.0, "prov": "I"},
        "storeys": {"range": [1, 2], "typical": 1, "prov": "D", "src": ["S10a", "S30:G1"]},
        "storey_height_m": {"floor_to_ceiling": [2.6, 3.0], "eaves_above_ground": [3.0, 3.4], "ridge_above_ground": [5.0, 5.5], "plinth_height": [0.4, 0.8], "prov": "P+I", "src": ["S30:G1"]},
        "bay_m": {"range": [2.0, 2.8], "prov": "I"},
        "wall_thickness_m": {"log": [0.18, 0.25], "frame_and_boards": [0.15, 0.20], "wattle_daub_or_adobe": [0.25, 0.40], "plinth": [0.4, 0.6], "prov": "I", "note": "mud-walled houses documented (S13, S1)"},
        "roof": {"type": "gabled or hipped standing-seam sheet iron, fretwork bargeboards", "pitch_deg": [25, 35], "prov": "P", "src": ["S30:G4"]},
        "cellar": {"share": 0.5, "kind": "root cellar (pogreb) under the floor or in the yard", "depth_m": [1.5, 2.2], "prov": "I"},
        "floor_construction": {"all": "timber on joists over the plinth", "prov": "I"},
        "stove_and_chimney": {"count": 1, "stove_base_m": [1.0, 2.0], "stack_section_m": [0.6, 1.5], "stack_top_above_ground": [6.0, 7.0], "prov": "D+P", "src": ["S21b", "S30:G13"]},
        "windows_m": {"w": [0.8, 1.0], "h": [1.2, 1.5], "shutters_or_surrounds": true, "prov": "I"},
        "doors_m": {"door": {"w": [0.8, 0.9], "h": [1.9, 2.0]}, "yard_gate": {"w": [2.5, 3.0], "h": [2.0, 2.5]}, "prov": "I"},
        "stairs": "porch steps 0.4-0.8 m; ladder to the loft"
      },
      "workshop": {
        "frontage_m": {"range": [8.0, 40.0], "typical": [10.0, 24.0], "prov": "I", "src": ["S13"]},
        "depth_m": {"range": [6.0, 16.0], "typical": [8.0, 12.0], "prov": "I"},
        "storeys": {"range": [1, 2], "typical": 1, "prov": "D+I", "src": ["S13", "S30:G1"]},
        "storey_height_m": {"eaves": [3.5, 5.5], "factory_edge_piers": [6.0, 8.0], "blank_wall": [3.0, 4.0], "prov": "P+I", "src": ["S30:G1"]},
        "bay_m": {"pier_spacing": [3.5, 6.0], "pier_width": [1.0, 1.3], "prov": "P+I", "src": ["S30:G3", "S29"]},
        "wall_thickness_m": {"exterior": [0.38, 0.51], "timber_shed": [0.05, 0.15], "prov": "I"},
        "roof": {"type": "gabled, lean-to or segmental; sheet iron or boards on timber (steel in larger sheds)", "pitch_deg": [10, 25], "prov": "P+I", "src": ["S30:G4"]},
        "cellar": {"share": 0.1, "prov": "I"},
        "floor_construction": {"ground": "earth or concrete", "roof_structure": "timber or steel trusses", "prov": "I"},
        "windows_m": {"w": [1.2, 2.0], "h": [1.5, 2.5], "sill_wall_h": [0.9, 1.2], "prov": "P+I", "src": ["S29"]},
        "doors_m": {"gate": {"w": [3.0, 4.0], "h": [3.0, 3.5]}, "door": {"w": [0.9, 1.2], "h": [2.0, 2.2]}, "prov": "I"},
        "stairs": "none, or a ladder"
      }
    },
    "condition_share_centre_jan_1943": {
      "prov": "I",
      "src": ["S1", "S2", "S3", "S5", "S28", "S10"],
      "merchant_brick_house": {"intact": 0.02, "burned": 0.55, "shelled": 0.18, "partly_collapsed": 0.25},
      "apartment_block": {"intact": 0.01, "burned": 0.45, "shelled": 0.24, "partly_collapsed": 0.30},
      "wooden_house": {"intact": 0.02, "burned": 0.90, "shelled": 0.03, "partly_collapsed": 0.05},
      "workshop": {"intact": 0.03, "burned": 0.50, "shelled": 0.17, "partly_collapsed": 0.30},
      "razed_plots_share_of_prewar_masonry": [0.05, 0.15],
      "meaning": {
        "intact": "roof and floors present",
        "burned": "roof and timber floors gone, walls full height, burned debris in the cellar; wooden house: plinth, stove and chimney only",
        "shelled": "breaches 1.5-3 m, pitting, some slabs and floors remain",
        "partly_collapsed": "a corner, end or section down to a rubble cone"
      }
    },
    "typology_mix_intro_district": {
      "prov": "I",
      "src": ["S1", "S5", "S13", "S14", "S28", "S30:L2"],
      "by_count": {"merchant_brick_house": [0.35, 0.45], "wooden_house": [0.25, 0.40], "apartment_block": [0.08, 0.12], "workshop": [0.10, 0.20]},
      "by_footprint_area": {"merchant_brick_house": [0.35, 0.45], "wooden_house": [0.10, 0.20], "apartment_block": [0.25, 0.35], "workshop": [0.10, 0.15]},
      "yard_sheds_per_plot": [1, 3],
      "street_frontage_occupancy": {"core": [0.75, 0.90], "ordinary": [0.60, 0.80], "fringe": [0.40, 0.60]}
    },
    "block": {
      "half_block_m": {"long": [94.0, 120.0], "short": [47.0, 75.0], "median": [97.0, 58.0], "full_range": [[64.0, 121.0], [33.0, 93.0]], "prov": "M", "src": ["S6"], "note": "hatched area on the 1942 plan; real block about 2-4 m larger"},
      "full_block_m": {"long": [150.0, 215.0], "short": [60.0, 116.0], "prov": "M", "src": ["S6", "S7"]},
      "street_pitch_m": {"across": [93.0, 126.0], "along_volga": [184.0, 235.0], "prov": "M", "src": ["S8", "S6"]},
      "old_fortress_quarter": "irregular blocks 25-40 x 80-120 m at an angle to the grid",
      "plot_depth_m": [30.0, 50.0],
      "plot_frontage_m": {"merchant": [15.0, 35.0], "wooden": [8.0, 12.0], "apartment": [40.0, 80.0]},
      "arrangement": "buildings on the street line; continuous brick street walls with party walls in the core; plots back to back; 1930s blocks as L/U perimeter or parallel slabs set back"
    },
    "streets": {
      "facade_to_facade_measured_m": [17.6, 29.0],
      "by_class_m": {"main_tram": [22.0, 30.0], "ordinary": [17.0, 22.0], "lane": [8.0, 14.0], "yard_passage_or_gate": [3.0, 4.0], "square": [60.0, 250.0]},
      "prov": "M+P+I",
      "src": ["S8", "S9", "S10p", "S30:L3", "S14"],
      "surface": "main streets asphalt or setts (asphalt from 1913), side streets unpaved earth",
      "clear_lane_between_rubble_berms_m": [10.0, 14.0]
    },
    "yards": {
      "depth_m": [20.0, 35.0],
      "width_m": [15.0, 35.0],
      "half_block_yard_depth_m": [12.0, 25.0],
      "courtyard_of_perimeter_block_m": [30.0, 60.0],
      "contents": ["timber sheds", "stores against yard walls", "summer kitchen", "stable or garage", "wooden annex with gallery stair", "well or pump", "root cellar"],
      "state_jan_1943": "outbuildings burned and gone; open ground with rubble and snow; trenches linking buildings",
      "prov": "D contents, I dimensions",
      "src": ["S10o", "S10o2", "S10r", "S1", "S9", "S13", "S28"]
    },
    "tram": {
      "gauge_m": 1.524,
      "rail": "heavy type 2A on treated pine sleepers, sand ballast (1913)",
      "tracks": "double on the main streets (masts between tracks outside the centre), single in side streets",
      "track_centres_m": [3.3, 3.8],
      "support_in_centre": "span wires between side poles",
      "pole_spacing_m": [30.0, 40.0],
      "contact_wire_height_m": [5.5, 6.0],
      "cars_1942": "two-axle cars, many blasted and burned out in the streets",
      "prov": "D (gauge, rail, masts, span wires, wrecks) + I (centres, spacing, height)",
      "src": ["S15", "S16", "S5", "S3"]
    },
    "poles_and_wires": {
      "timber_pole_height_m": [7.0, 9.0],
      "crossarms": [1, 4],
      "state": "many leaning or broken; wires sagging and hanging; fallen poles and wires across streets",
      "prov": "P+D",
      "src": ["S30:L4", "S3"]
    },
    "fences_and_walls": {
      "picket_fence_h_m": [0.8, 1.1],
      "board_fence_h_m": [1.6, 2.0],
      "garden_wall_h_m": [0.6, 1.0],
      "yard_wall_h_m": [2.2, 3.0],
      "yard_wall_thickness_m": [0.38, 0.51],
      "street_wall_with_pilasters_h_m": [2.5, 3.0],
      "gate_w_m": [2.5, 3.5],
      "prov": "P+D+M+I",
      "src": ["S30:L5", "S10o", "S10g", "S29"]
    },
    "rubble": {
      "bulking_factor": 1.5,
      "berm_along_gutted_building_h_m": [1.0, 2.0],
      "slope_against_4_5_storey_ruin_h_m": [1.5, 3.0],
      "cone_at_collapsed_section_h_m": [3.0, 5.0],
      "mound_h_m": [1.0, 2.0],
      "flattened_one_storey_h_m": [0.3, 1.5],
      "ridge_from_fallen_facade_h_m": [2.0, 4.0],
      "burned_shell_interior_fill_h_m": [0.3, 1.0],
      "angle_of_repose_deg": [35.0, 40.0],
      "composition": "loose brick with bonded lumps 0.2-0.6 m, beams, rods, pipes, roof sheet",
      "prov": "P + I (mass balance) + D (qualitative)",
      "src": ["S30:G17", "S2", "S5"]
    },
    "craters": {
      "city_count": 150000,
      "mean_density_per_km2": [750, 1900],
      "diameter_m": [1.0, 10.0],
      "large_bomb_craters": "one-ton bombs, some reaching groundwater",
      "prov": "D count and qualitative; I density and diameters",
      "src": ["S2", "S5"]
    },
    "snow_and_weather": {
      "mean_temp_c": {"1942-12": -10.3, "1943-01": -8.4, "1943-02": -2.1},
      "annual_precipitation_mm": 280,
      "snow_open_ground_m": [0.05, 0.15],
      "snow_squares_m": [0.2, 0.3],
      "snow_drift_in_roofless_interiors_m": [0.3, 0.8],
      "prov": "D (temperatures, precipitation) + P (snow)",
      "src": ["S24", "S23", "S30:L8"]
    },
    "tank_constraints": {
      "tiger_width_m": [3.56, 3.70],
      "tiger_length_m": {"hull": 6.316, "gun_forward": 8.45},
      "tiger_height_m": 3.0,
      "ground_clearance_m": 0.47,
      "max_slope_deg": 35,
      "trench_m": 2.5,
      "roof_armour_mm": 25,
      "gun_elevation_deg": [-8, 15],
      "game_trunnion_height_m": 2.37,
      "game_muzzle_radius_from_turret_axis_m": 5.83,
      "game_collision_box_m": [4.12, 7.13],
      "min_clear_lane_m": 5.0,
      "full_traverse_clear_width_m": 11.7,
      "max_hit_height_m_at_distance": {"10": 5.0, "15": 6.4, "20": 7.7, "30": 10.4, "45": 14.4},
      "min_hit_height_m_at_distance": {"5": 1.67, "10": 0.97, "15": 0.26},
      "distance_to_reach_m": {"3rd_storey_sill_8.5m": 23, "4th_storey_sill_11.8m": 35, "parapet_15m": 47, "street_level": 17, "semi_basement_sill_0.3m": 15},
      "firing_positions_documented": ["semi-basement windows along streets", "lower floors: direct-fire guns and heavy MGs", "upper floors and attics: fire down on tanks", "roofs behind parapets", "breaches in fences and walls, doorways, gates, windows (ambush)"],
      "prov": "D (vehicle, positions) + I (geometry)",
      "src": ["S25", "S31", "S3", "S12"]
    },
    "recommended_grammar_limits_4m_bay": {
      "prov": "I",
      "merchant_brick_house": {"width_m": [12.0, 40.0], "depth_m": [8.0, 16.0], "storeys": [1, 3], "cellar_allowed": true},
      "apartment_block": {"width_m": [24.0, 72.0], "depth_m": [12.0, 16.0], "storeys": [3, 6], "cellar_allowed": true},
      "wooden_house": {"width_m": [8.0, 12.0], "depth_m": [8.0, 8.0], "storeys": [1, 1], "cellar_allowed": true},
      "workshop": {"width_m": [8.0, 36.0], "depth_m": [8.0, 16.0], "storeys": [1, 1], "cellar_allowed": false},
      "note": "real merchant depths of 9-12.5 m quantise to 8 or 12 m; 1930s window axes of ~3 m suggest a 3-3.3 m sub-bay inside the 4 m module"
    }
  }
}
```
