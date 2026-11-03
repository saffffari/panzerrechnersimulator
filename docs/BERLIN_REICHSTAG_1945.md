# Berlin 1945: the Reichstag and the last stand (map provenance)

The workshop `berlin-reichstag-v1` (generator `tools/worldgen/berlin_reichstag.py`, models
`tools/berlin1945/`) is the Reichstag and a few blocks of central Berlin as they stood between
28 April and 2 May 1945: the Reichstag, Koenigsplatz with its flooded cut and trenches, the Kroll
Opera, the Ministry of the Interior by the Moltke Bridge, the Swiss legation, the Spree, the
Brandenburg Gate with Pariser Platz and the Adlon, the start of Unter den Linden, the Tiergarten
with the Charlottenburger Chaussee, the Lehrter Bahnhof and the Humboldthafen across the river.

This file records what each part of the map rests on, so a reader can check it, and where it is
an estimate. It follows CLAUDE.md: the scene is the attested place and its condition; the
player's tank, route and order are not a record of a unit or an engagement.

Compiled 2026-11-02. No photograph is copied into the repository; the references below are links.
Camera directions marked *(reading)* are an interpretation of the picture, not archive data.

## 1. The photographs the models were checked against

All Wikimedia Commons pages were confirmed through the Commons API (author, date, licence).

### The Reichstag

| Ref | Photograph | Author, date, licence | What it shows, from where |
|---|---|---|---|
| R1 | [Ruins of the Reichstag in Berlin, 3 June 1945. BU8573](https://commons.wikimedia.org/wiki/File:Ruins_of_the_Reichstag_in_Berlin,_3_June_1945._BU8573.jpg) | Sgt Hewitt, No 5 AFPU (IWM BU 8573), 3 June 1945, public domain (expired Crown copyright) | From Koenigsplatz at ground level, a little south of the axis, looking east at the west portico *(reading)*: six Corinthian columns, the pediment relief, the inscription with letters shot away (it reads "DEM EUTSCHEN OLKE"), the bare steel ribs of the dome, the Germania group above the portal, windows walled up with small firing slits, free-standing brick blast walls before the portico. The main elevation reference. |
| R2 | [Здание Рейхстага, разрушенное во время штурма...](https://commons.wikimedia.org/wiki/File:Здание_Рейхстага,_разрушенное_во_время_штурма_города_советскими_войсками.jpg) | Yevgeny Khaldei, May 1945, public domain | The west front from the rubble of Koenigsplatz, west-north-west, looking east-south-east *(reading)*: the north-west tower at left, flat-topped; the portico; an IS-2. The bomber formation is probably a darkroom addition. |
| R3 | [Танки ИС-2 у Рейхстага](https://commons.wikimedia.org/wiki/File:Танки_ИС-2_у_Рейхстага.jpg) | Viktor Temin (TASS), 2 May 1945, public domain | IS-2 No. 434 by a flat-topped corner tower with the dome frame behind; equestrian figures on the roof line (probably the east side's heralds). |
| R4 | [Фото "Рейхстаг" (184813 5)](https://commons.wikimedia.org/wiki/File:Фото_"Рейхстаг"_(184813_5).jpg) | Khaldei, May 1945, public domain | The street along the east front (Hermann-Goering-Strasse / Sommerstrasse) looking north *(reading)*: colonnade, a herald on the roof corner, a tram. |
| R5 | [Советская штурмовая группа со знаменем движется к рейхстагу](https://commons.wikimedia.org/wiki/File:Советская_штурмовая_группа_со_знаменем_движется_к_рейхстагу.jpg) | Ivan Shagin (TASS), May 1945, CC BY 4.0 | Arched ground-floor bays bricked up with small square loopholes (probably restaged after the fighting). |
| R6 | [Bundesarchiv Bild 204-008](https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_204-008,_Berlin,_Henryk_Gorovits_vor_Reichstag.jpg) | unknown, May 1945, CC BY-SA 3.0 de | Rusticated ground storey, windows infilled with masonry, rubble at the base. |
| R7 | [Berlin- the Capture and Aftermath of War 1945-1947 BU8922](https://commons.wikimedia.org/wiki/File:Berlin-_the_Capture_and_Aftermath_of_War_1945-1947_BU8922.jpg) | Capt E G Malindine, AFPU (IWM BU 8922), 12 July 1945, public domain | The south front with both south towers (flat-topped) and the dome frame, from the south across a paved road with tram tracks *(reading)*. |
| R8 | [Raising a flag over the Reichstag](https://commons.wikimedia.org/wiki/File:Raising_a_flag_over_the_Reichstag.jpg) | Khaldei, 2 May 1945, public domain | The staged flag photograph from the roof parapet beside a corner tower. The direction is not established; not used for geometry. |

Also used for proportions: the Commons category
[Photographs by Yevgeny Khaldei at the Reichstag](https://commons.wikimedia.org/wiki/Category:Photographs_by_Yevgeny_Khaldei_at_the_Reichstag)
and the USAAF film [SFP 186 - Reichstag](https://commons.wikimedia.org/wiki/File:SFP_186_-_Reichstag.ogv) (July 1945).

### The Brandenburg Gate

| Ref | Photograph | Author, date, licence | What it shows, from where |
|---|---|---|---|
| B1 | [Brandenburger Tor-2](https://commons.wikimedia.org/wiki/File:Brandenburger_Tor-2.jpg) | Sgt Malcolm Thurgood (Alsos Mission), 1945, AIP Emilio Segre Visual Archives, CC0 | The west face, frontal, from the Platz vor dem Brandenburger Tor looking east (the Commons caption says "from the east"; the picture shows the camera stood west): the six columns, the wrecked Quadriga, a shell hole in the attic, the gatehouses' colonnades either side, Pariser Platz ruins through the passages. |
| B2 | [Reichstag Building and Brandenburger Tor](https://commons.wikimedia.org/wiki/File:Reichstag_Building_and_Brandenburger_Tor.jpg) | Thurgood, 1945, CC0 | From the south along Hermann-Goering-Strasse looking north: the gate and its south gatehouse at right, the Reichstag's south front at left. The gate-to-Reichstag relationship. |
| B3 | [Bundesarchiv Bild 183-M1015-331](https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-M1015-331,_Berlin,_zerstörtes_Brandenburger_Tor_und_Reichstagsgebäude.jpg) | Otto Donath, 1945, CC BY-SA 3.0 de | Elevated, from the Pariser Platz side looking north-west *(reading)*: attic and frieze, the Quadriga wreck carrying a flag, the south gatehouse's pediment (left), the north gatehouse's colonnade (right), the Reichstag beyond. |
| B4 | [Bundesarchiv Bild 183-M1015-334](https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-M1015-334,_Berlin,_Brandenburger_Tor.jpg) | Donath, 1945, CC BY-SA 3.0 de | From the rubble of Pariser Platz: the burnt-out gatehouses with their Doric fronts and pediments standing, the Quadriga stump. |
| B5 | [Фото "Вид с Бранденбургских ворот" (184715 5)](https://commons.wikimedia.org/wiki/File:Фото_"Вид_с_Бранденбургских_ворот"_(184715_5).jpg) | Khaldei, May 1945, public domain | From the top of the gate beside a Quadriga horse looking east down Unter den Linden: a bullet-pocked horse head and neck still in place; Pariser Platz frontages, some still roofed. |
| P3 | [Bundesarchiv Bild 146-1998-010-21](https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_146-1998-010-21,_Berlin,_Pariser_Platz,_Luftaufnahme.jpg) | Klinke & Co., October 1931, CC BY-SA 3.0 de | Oblique aerial looking east over the intact gate, both gatehouses, Pariser Platz, the Adlon at the south-east corner and the Linden. |

### The other places

| Ref | Photograph | Author, date, licence | What it shows |
|---|---|---|---|
| K1 | [Bundesarchiv Bild 183-M1015-322](https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-M1015-322,_Berlin,_zerstörte_Krolloper_am_Platz_der_Republik.jpg) | Donath, 1945, CC BY-SA 3.0 de | A wrecked heavy flak gun on its cruciform platform (archive caption "zerstörtes Flakgeschütz") before the gutted Kroll: a two-storey front block with arched windows, the stage house's gable with its great arched opening, stripped and shattered trees. |
| K3 | [Berlin-Tiergarten, Platz der Republik (Königsplatz), Ansichtskarte](https://commons.wikimedia.org/wiki/File:Berlin-Tiergarten,_Platz_der_Republik_(Königsplatz),_Ansichtskarte.tif) | postcard, 1939 at the latest, public domain | Koenigsplatz from the Reichstag looking west before the 1938-39 removals: the Kroll, the Victory Column, the Bismarck monument. |
| T1 | [Bundesarchiv Bild 183-M1015-321](https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-M1015-321,_Berlin,_das_zerstörte_Restaurant_"Zelte"_im_Tiergarten.jpg) | Donath, 1945, CC BY-SA 3.0 de | The burnt-out restaurant row "In den Zelten" with broken trees. |
| A1 | [AWM P04054.006](https://www.awm.gov.au/collection/C1018250) | 1945, public domain | Aerial looking west over the gate, the ruined Pariser Platz and the burnt-out Reichstag, the Spree and the Lehrter Bahnhof beyond. |
| A2 | [AWM SUK14267](https://www.awm.gov.au/collection/C280527) | RAF FPU, c. May 1945, public domain | Low aerial views of Berlin after its fall. |

Not found: a 1945 photograph of the Ministry of the Interior (the former General Staff building),
of the Swiss legation, of the Moltke Bridge, or of the flooded cut on Koenigsplatz. Those parts
rest on plans, the aerial survey and the accounts (sections 2-4).

## 2. Plans and the frame

- **The frame.** The map's origin is the centre of the Reichstag's footprint, OpenStreetMap
  relation 2201742 (52.51859575 N, 13.3761008 E); x metres east, n metres north (engine z = -n);
  a local equirectangular projection (error under 0.1 m over the map). The workshop is a 1.8 km
  square.
- **OpenStreetMap** (data (c) OpenStreetMap contributors, ODbL 1.0; snapshot recorded in the
  receipt; fetched by `tools/berlin1945/fetch_osm.py` into the asset root, not the repository):
  the Spree's banks and centreline, the Humboldthafen, the street lines of the Dorotheenstadt and
  of the Tiergarten's avenues, the Reichstag's outline (the Wallot walls survived both rebuilds),
  the Brandenburg Gate (way 518071791) and its gatehouses (ways 518071762, 375247938), the
  Reichstag President's palace (way 37408952), the Swiss legation (relation 2886766, historic
  part), the Adlon's corner (way 4582978), the Moltke Bridge's axis (way 34833363), and the plots
  of today's buildings where the old blocks were rebuilt on their old lines.
- **The 1928 aerial survey** of Berlin, 1:4,000 (Geoportal Berlin WMS `luftbild_1928_04`,
  Datenlizenz Deutschland Zero 2.0), resampled into the map's frame: Koenigsplatz's 1928 layout
  (the ring road about 96 m in radius round the column site at (-273, -5), six radial paths), the
  Kroll (about 75 x 110 m at (-520, -1)), the General Staff / Ministry block (about 93 x 156 m with
  its north-east side on the diagonal street from the bridge), the Lehrter Bahnhof's shed and the
  Humboldthafen's basin. The mosaic sits about 6 m north of OSM at the Reichstag; traced
  positions are corrected by that.
- **The 1953 aerial survey** (WMS `luftbild_1953_22`, same licence): checked for what survived
  and for the cut's trace; its georeferencing is off by 20-30 m here, so it is not used for
  positions.
- **The Ivengo battle map** ([Battle for Reichstag 1945 map-eng.png](https://commons.wikimedia.org/wiki/File:Battle_for_Reichstag_1945_map-eng.png),
  2010, CC BY-SA 3.0): a modern schematic, not a period source. The line of the flooded cut
  across the west half of Koenigsplatz and the German positions (the Swiss legation, the ministry,
  the Kroll, the trenches on the square, the Reichstag's perimeter) follow it.

## 3. Dimensions and architecture used

### Reichstag (`tools/berlin1945/model_reichstag.py`)

| Item | Value | Source / status |
|---|---|---|
| Footprint | 137.8 m north-south over the corner pavilions, 104 m east-west over the west portico and the east portal; main walls at x = +-43.3 m | OSM relation 2201742 (de.wikipedia gives 137 x 104 m and elsewhere 138 x 96 m) |
| Corner pavilions | about 18 x 21 m, towers 12 m square | OSM outline; tower plan estimated from R2 |
| Towers | flat tops at 41.2 m with a parapet to 42.4 m | 46 m with the caps (de.wikipedia); the caps were removed in 1941 for 2 cm flak platforms (flat in R1, R2, R3, R7); height is an estimate |
| Eaves, balustrade | entablature top 25.2 m, balustrade 26.8 m; rusticated ground storey to 8.4 m | scaled from R1, R2, R7: an estimate |
| West portico | six Corinthian columns (counted in R1), a second row of four behind, podium 7.4 m with a 24 m stair, pediment rising 6.4 m, the Germania group on the portal attic | R1; the stair's depth is an estimate |
| Inscription | "DEM DEUTSCHEN VOLKE", 60 cm bronze letters (Behrens design, Loevy foundry, December 1916), modelled as R1 shows it: the D of DEUTSCHEN and the V of VOLKE gone | de.wikipedia; R1. The letterforms are Blender's default font, not Behrens' |
| Dome | octagonal (square with cut corners) drum 34 m across, 37.6 m high; the cupola's bare steel ribs (40 meridians, 10 rings, about one rib section in fourteen blown out) to 55 m, the lantern frame bent and partly gone, a spike to about 70 m; four corner turrets with stone crowns | "knapp 75 m" to the top (de.wikipedia); the octagonal ring (de.wikipedia); R1, R2, R7 for the bare frame; rib counts are an estimate |
| East side | portal projection with four engaged columns, the two copper equestrian heralds (Maison, 1896) at its corners | de.wikipedia; R3, R4 |
| 1945 state | every window bricked up with a firing slit (1939; R1, R5, R6), some infills blown, brick blast walls before the portico (R1), soot over the windows and on the towers, cornice and balustrade pieces shot off, rubble against the walls | photographs; the share of damage is an estimate |
| Not modelled | the interior (the burnt plenary hall, the rooms fought through 30 April - 2 May) | collision is the solid mass of the wings behind the bricked windows |

### Brandenburg Gate (`tools/berlin1945/model_brandenburg_gate.py`)

| Item | Value | Source |
|---|---|---|
| Gate | 34.8 m along the colonnade, 11 m deep; twelve fluted Doric columns 13.5 m high, 1.73 m at the foot; passages 5.65 m (centre) and 3.80 m; entablature to 17.65 m, stepped attic to 20.3 m | de.wikipedia, en.wikipedia; OSM way 518071791 |
| Overall | 61 m with the gatehouses (62.5 m given) | de.wikipedia; OSM |
| Gatehouses | 18.6 x 13 m each, east of the gate's ends: an open Doric colonnade on the side toward the passages, a four-column front with a pediment toward Pariser Platz, four columns toward the Tiergarten; burnt out, roofless, the pediments standing | OSM ways 518071762 / 375247938 (the 1990s rebuilds on the old footprints); B1, B3, B4; heights are an estimate |
| Turn | 6.5 deg: the gate faces down Unter den Linden (bearing 83.5 deg) | OSM |
| Quadriga | wrecked: the chariot's shell, four horses broken at the body or the legs, one head and neck still up, facing east | B3, B4, B5. The Soviet flag seen on it in B3 and B4 is not modelled: those photographs are not dated within the battle |
| Damage | a shell hole in the west attic north of the centre, cornice pieces gone, metopes knocked out, splinter scars on the columns | B1; the rest an estimate |

### The other models (`tools/berlin1945/model_landmarks.py`, `model_city_blocks.py`, `model_props.py`)

- **Kroll Opera**: front block on the square, side wings, the auditorium and the stage house
  with its gable and arched opening, all gutted (K1; the November 1943 raids and 30 April 1945,
  de.wikipedia Krolloper). Plan from the 1928 survey; heights are an estimate.
- **Ministry of the Interior** (the former General Staff building): a four-storey block, gutted,
  the south wing partly collapsed; the north-east wing on the diagonal street from the bridge.
  Taken room by room by the 150th Rifle Division on 29 April (Beevor 2002, via en.wikipedia
  Battle in Berlin). That this is Beevor's "Himmler's house" is an inference. Elevations are an
  estimate (no photograph found).
- **Swiss legation**: three storeys and a mansard, damaged, sandbagged; the one Alsenviertel
  building to survive (de.wikipedia Alsenviertel); held as a strongpoint (Ivengo map).
- **Reichstag President's palace**: three storeys and a mansard, roofed, damaged.
- **Hotel Adlon**: five storeys and a mansard, intact: it was undamaged at the surrender on
  2 May and burned in the following days while occupied by Soviet soldiers, cause unexplained
  (de.wikipedia Hotel Adlon). Its size is the old Adlon's corner, an estimate (48 x 40 m).
- **Moltke Bridge**: red Main sandstone, three river arches (16.3, 16.3, 17 m), 78 m by 26.7 m
  (de.wikipedia Moltkebruecke); barricaded at both ends; the southern river arch blown on
  28 April, its downstream two thirds in the Spree, the upstream third still spanning: "passable
  to infantry" (Beevor 2002 pp. 340, 347-349 via en.wikipedia Moltke Bridge). The span widths
  are sourced; which part of the arch fell is an estimate.
- **Lehrter Bahnhof**: the shed's walls and bare roof arches, the head building partly fallen;
  plan from the 1928 survey; the 1945 state is the district's, not a photograph's.
- **City blocks**: three types (a five-storey rendered tenement or commercial house, a
  three-storey stone palais, a five-storey stone office or hotel building), 14 or 24 m wide, in
  three conditions (gutted, collapsed, roofed and damaged), placed on the plots of the old blocks
  (OSM plots outside the areas rebuilt on a new plan), the frontage on the street. In the blocks
  round the Reichstag and Pariser Platz every plot stands; further out about a third stand and
  the rest are rubble heaps. The proportions are an impression from the photographs (B5, A1), not
  a survey of individual buildings: the map does not claim which house stood or fell.
- **Grammar houses**: the burnt row on In den Zelten (T1), the building grammar's
  merchant houses and an apartment block, burned or partly collapsed.
- **Shattered trees** and the **wrecked flak gun**: K1, T1.

## 4. The fighting's traces, and what is not shown

Sources: Antony Beevor, *Berlin: The Downfall 1945* (2002), as cited in
[en.wikipedia Battle in Berlin](https://en.wikipedia.org/wiki/Battle_in_Berlin) and
[Moltke Bridge](https://en.wikipedia.org/wiki/Moltke_Bridge); Hamilton (2008) for the flooding;
de.wikipedia (Reichstagsgebaeude, Platz der Republik, Alsenviertel, Strasse des 17. Juni).

- **28/29 April**: the 3rd Shock Army's 79th Rifle Corps (150th and 171st Rifle Divisions)
  crossed the Moltke Bridge; the bridge's charges damaged it; by midnight the bridgehead held.
  In the map: the barricades at both ends, the blown arch.
- **29 April**: the Ministry of the Interior taken room by room. In the map: the ministry gutted,
  its south wing collapsed, sandbags on its south front.
- **30 April**: attacks across Koenigsplatz from 06:00 under crossfire from the Reichstag and the
  Kroll and 12.8 cm fire from the Zoo flak tower; the troops reached the water-filled cut by
  about 10:00-11:00 ("a collapsed tunnel had filled with water from the Spree forming a moat",
  Beevor; Hamilton blames locks broken by shelling); the main assault at 18:00; room-to-room
  fighting. In the map: the cut, 7 m across and 3.4 m deep with water 1.6 m below the square
  (the width is an estimate; one secondary source gives about 3 m), the trenches and gun pits on
  the square and round the Reichstag, the craters, and the dead of the assault in the open by the
  cut and the trenches.
- **Defenders**: about 1,000 in the Reichstag, sailors, SS and Hitler Youth; about 300
  surrendered, about 200 dead (Beevor). The map shows no named person and no particular death.
- **Surrender**: the evening of 1 May (Beevor) or 2 May (commonly given): unresolved.
- **Flags**: the flag raisings are disputed (Moscow's 22:50 on 30 April; earlier claims;
  Khaldei's staged photograph of 2 May). No flag is placed on the Reichstag or the Gate.
- **Knocked-out armour** (a T-34-85 and an IS-2 on the square, burnt lorries) and the **sandbags,
  hedgehogs and barricades** on the approaches are recurring conditions of the battle, not
  records of particular vehicles or positions.
- **The Charlottenburger Chaussee** was widened to up to 45 m (1937-39) and used as an airstrip
  in the last days, the trees by it felled (de.wikipedia Strasse des 17. Juni). The map clears
  the trees from its verges near the gate.
- **The Tiergarten** in April 1945 was still wooded, shell-torn; it was felled for fuel in the
  winter of 1945-46 (T2 below shows the result in July 1946, so it is not used for 1945).
  T2: [Bundesarchiv Bild 183-M1015-314](https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-M1015-314,_Berlin,_baumloser_Tiergarten.jpg).
- **Koenigsplatz**: the Victory Column and the Bismarck monument went to the Grosser Stern in
  1938-39; the square in 1945 is open lawn, paths and the ring road.
- **The Alsenviertel** between the ministry and the Reichstag was demolished 1939-42 for
  Speer's planned hall: cleared ground in 1945, drawn as sand and rubble.

## 5. Approximations and omissions (see also docs/PHYSICAL_TRUTH_AUDIT.md, Berlin 1945 rows)

- The city's houses are three generic types on the old plots, not the individual buildings.
- The weather and hour (30 April 1945, 17:30 local, overcast with smoke haze) are a choice; the
  day's actual weather was not researched.
- Bridges other than the Moltke Bridge (the Kronprinzen-, Marschall-, Weidendammer and the
  Stadtbahn bridges), the Stadtbahn viaduct over the Humboldthafen, the Tiergarten's ponds and
  paths, the street lamps (Speer's on the East-West axis), tram wires and the New Reich
  Chancellery (outside the map's few blocks) are not modelled.
- A tank cannot cross the Spree on this map: the Moltke Bridge is barricaded (as it was), and
  the engine's tank-bearing bridges (`src/river_crossings.rs`) run along world X only, which no
  bridge here does.
