# Wartime photographs as 3D reference: Act 2 (the Oderbruch) and Act 3 (Halbe)

29 September 2026. Reference set: `/home/alex/panzer/reference/wartime_photos_1945_acts_20260929/`
(README.md, manifest.json, index.html). Modelled on the urban-intro analysis
`docs/research/PHOTO_REFERENCE_ANALYSIS_20260928.md` in the repository.

## Summary

All 96 downloaded photographs were looked at one by one on 600 px review sheets (and the key ones at
full size), and each is catalogued in sections 2 (Act 2, 48) and 3 (Act 3, 48) with its source, date,
place, reliability grade, what it shows and what the 3D world must reproduce. Sections 4 and 5 turn them
into 52 numbered rules: buildings (B2, B3), terrain and vegetation (T2, T3), dressing and wrecks (D2, D3),
weather and light (W2, W3) and the physical traces of human events (H2, H3). Section 6 names the
photographs each environment must be compared against; section 7 lists what the set cannot tell.

The set's limit comes first: **13 photographs were taken on the acts' own ground, 7 more come from the
same operation with the place unverified, and 76 are labelled close equivalents.** Act 2 is reasonably
anchored (Podelzig and Reitwein after the battle, the Oder at Frankfurt and Küstrin, the command posts of
16 April). Act 3 is not: one weak-provenance photograph of the pocket, and every other rule for Halbe rests
on equivalents.

What matters most:

1. **The leveled home is a low rubble field under dead trees**, not standing shells: 0.5-1 m of broken
   masonry over each footprint, chimney stacks and a church gable or tower shaft as the only verticals,
   and a skyline of shell-stripped trees taller than the ruins (B2-2, B2-3, H2-1: [a2-05-01],
   [a2-01-01], [a2-05-03]).
2. **The Oderbruch is dead flat and bare in April**: ploughed earth that keeps every track, tree lines
   at 1-3 km as the only relief, avenues of big trees along the roads (T2-1, T2-2, T2-4: [a2-02-02],
   [a2-02-05], [a2-06-01]).
3. **The Oder's meadows were flooded**: water to the eaves of low buildings, dikes as the only dry lines,
   a river 150-250 m wide, mist over it (T2-3, W2-1: [a2-03-02], [a2-03-05], [a2-03-01]).
4. **Crossings and bridges fail in characteristic ways**: low pile trestles, blown trusses lying in the
   river, canal decks dropped between intact abutments, small bridges collapsing under a tank (D2-1,
   D3-5: [a2-03-03], [a2-03-01], [a2-03-09], [a3-03-03]).
5. **The Halbe forest is Scots pine on sand**: straight or crooked trunks bare for most of their height,
   open floor, 30-80 m sight lines, pale sand in every cut, straight rides; birches in first leaf by late
   April (T3-1 to T3-4: [a3-01-01], [a3-01-04], [a3-01-09]).
6. **The breakout's end is a forest road of mixed wrecks under intact pines**, civilian and military,
   with their contents spilled ([a3-05-01], D3-1, H3-1, H3-2) - the act's key scene reference, with weak
   provenance.
7. **The Autobahn is a rampart**: concrete slabs, verges, overbridges and a 4-5 m embankment into which a
   blocking line digs its foxholes (T3-5, D3-5: [a3-03-06], [a3-03-01], [a3-03-02]).
8. **Smoke and mist are the atmosphere of both acts** and must come from the participating medium and the
   fires as emitters: contrast gone at about 50 m in burning-village smoke, backlit smoke glowing, night
   barrages lit only by their rockets (W2-1 to W2-3, W3-1).
9. **Traces, not bodies**: treks leave wagons, bundles, suitcases, handcarts and a child's cart; defenders
   leave Panzerfaust tubes and revetted positions; attackers leave gun pits, boxes and ruts (H2, H3). No
   photograph in the set shows the dead, and none should be invented from it.
10. **The set is silent on** Halbe's own streets, the A13 blocking line, s.SS-Pz.Abt. 502's Tigers, the
    Seelow scarp as a landform, the Oderbruch canals, and the windmill and water tower landmarks (§7).

Review sheets (internal reference only; several photographs are CC BY-SA) are in `_work/sheets/`:
`qa_<act>_<folder>_NN.jpg` show every photograph with its analysis id; `python3 _build/qa_sheets.py`
rebuilds them and `--ids` prints the id table. The screening sheets of candidates are there too
(`ca_`, `cb_`, `cc_`, `cd_`).

## 1. Method

### 1.1 What was looked at

Every downloaded record in `manifest.json` was opened and read against its caption, date, place,
photographer and archive id: first on 330 px screening sheets, then on 600 px review sheets (3 x 2),
and at full size where a detail or a measurement needed it. Photographs are identified as
`[aN-FF-NN]`: act, folder number and position inside the folder in `manifest.json` order
(`python3 _build/qa_sheets.py --ids` prints the table). The three link-only records are not
analysed; their captions and host pages are in the manifest.

The set is built around one hard fact: **open archives hold very few photographs of the acts' own
ground.** The Oderbruch battle and the Halbe pocket were photographed mostly by Soviet front
photographers whose prints sit in Russian archives, museum collections and private hands; the German
side took almost nothing in the last weeks. What Wikimedia Commons holds for the exact places is:
six Otto Donath (ADN) views of Podelzig and Reitwein after the battle; a NARA aerial of Küstrin;
two Bundesarchiv PK photographs and one private scan of the Oder at Frankfurt; two command posts of 16
April; one photograph of the Halbe pocket - 13 in all. Seven more (Soviet Oder crossings, gun lines and
tank columns of the Berlin operation) belong to the same operation and month but do not state the place:
they are marked **Place unverified**. Everything else (76) is a **labelled close equivalent**, and the
`location_match` field says so, with the reason. The rules below keep the classes apart: a rule resting
only on equivalents says so, and the owner should replace it with a cited in-region source when one
exists.

### 1.2 Reliability grades

| Grade | Meaning |
|---|---|
| A | Archive record (Bundesarchiv, IWM, NARA, Russian state catalogue) with photographer, a day or month and a place. PK and ADN captions keep their propaganda wording; the photograph is still documentary. |
| A- | Archive record whose date is a month or year, or whose place is a region only. |
| B | Named Soviet photographer with month and a region or front, via waralbum.ru, russiainphoto.ru, mil.ru or a newspaper; or an archive record whose caption is internally inconsistent. |
| C | Book or website scan, anonymous, 'ca.' date, or place and unit supplied by the uploader. Use for form only. |

Flags: **[prop]** propaganda caption; **[staged?]** arranged for the camera; **[winter]** taken in
January-February (frost, no snow cover or snow) and therefore not an April reference for vegetation and
ground; **[eq]** close equivalent from outside the act's area; **[map]** a vertical photomap, not a
photograph at eye level; **[print]** retouching or print contrast makes the tones unreliable. **In
region** and **Place unverified** mark the other two location classes (§1.1).

### 1.3 Measuring

Scales come from known objects: a standing man 1.70-1.80 m; a door 2.0-2.2 m; a German brick course of
about 77 mm (Reichsformat 250 x 120 x 65 mm with a 12 mm joint); a T-34-85 3.0 m wide and 2.7 m tall
to the turret roof; an IS-2 3.1 m wide; a Tiger II 3.75 m wide on battle tracks, 3.1 m tall, hull
7.4 m long; a Reichsautobahn carriageway of 7.5 m (two 3.75 m lanes) with a 5 m central reservation
(the 1930s Reichsautobahn standard); a motor car about 1.6 m wide. Every figure carries its uncertainty;
a figure read from a single reference at a different depth is +/-30 % at best.

### 1.4 Tone, not albedo

Every photograph is a black-and-white or sepia print or a scan of one (the NARA aerial is a vertical
reconnaissance frame, and the Bild 196 photomaps are photographic mosaics with printed grids). They inform form, damage, composition, relative
tone and the distribution of light. They never set albedo: film response, print contrast, retouching
(several Soviet prints are retouched) and scanning all change grey values. Spring colour, leaf-out and
soil tone come from the Sentinel-2 imagery already in `assets/terrain/act2` and `act3` and from physical
material data.

## 2. Catalogue, Act 2: the Oderbruch below the Seelow Heights

Each entry: path, source and archive id, date and place with a grade, what it shows, what the 3D world
must reproduce, scale readings with their uncertainty, and its use for the act.

### 2.1 Villages and farmsteads (`a2_01`, 7)

**[a2-01-01]** `a2_01_villages_farmsteads/a2-01_podelzig-ruined-village-and-field-work_barch-183-n0816-313.jpg`
Bundesarchiv Bild 183-N0816-313, Otto Donath (ADN), 1945, Podelzig, Kreis Lebus. Grade A- (year only;
the print strip and the caption both say 1945). **In region.** [prop] (ADN wording).
Shows: four villagers hoeing a rough meadow; 60-120 m behind them the village as a line of roofless,
white-plastered masonry shells and broken gables with the church ruin among them; dozens of trees with
their crowns shot away, standing as trunks with a few snapped limbs. Flat ground to the horizon.
Reproduce: a leveled Oderbruch village reads from its fields as a low, broken, pale band 3-8 m high
with no roofs, under a skyline of dead trees taller than the ruins (trees 10-18 m, +/-25 %, against the
people). The fields run straight up to the ruins without a fence line.
Use: the Act 2 destination (the leveled home), overview composition.

**[a2-01-02]** `a2_01_villages_farmsteads/a2-01_podelzig-barrack-used-as-village-school_barch-183-n0816-309.jpg`
Bundesarchiv Bild 183-N0816-309, Otto Donath, 1945, Podelzig. Grade A-. **In region.** [prop]
Shows: a single-storey timber barrack about 30 m long (+/-20 %, from the door), tar-paper roof at a low
pitch, six small brick chimneys along the ridge, small four-pane windows, boarded gable ends; weeds,
loose planks; dead trees behind with shattered crowns.
Reproduce: the standard wartime hut (RAD/labour-camp type) as a surviving building among ruins: timber
walls on a low plinth, stove chimneys every 4-5 m, windows about 1.0 x 1.0 m. After the battle it housed
the school; before it, it may have housed soldiers or forced labourers - the caption does not say.
Use: village dressing; one of the few roofed buildings in a leveled village.

**[a2-01-03]** `a2_01_villages_farmsteads/a2-01_soviet-infantry-in-brandenburg-village-street_1945-03.jpg`
Vladimir Grebnev (TASS), March 1945, "in the German province of Brandenburg", 1st Belorussian Front,
published 24 March 1945. Grade B (region and month; locality not named). **[eq]** (Brandenburg province,
probably the Neumark east of the Oder, where the 1st Belorussian Front stood in March). [prop]
Shows: Soviet infantry running along a village street: on the left a two-storey red-brick public
building (school or office) with round-arched windows and a round-arched door, rendered string courses;
brick yard walls and gateposts; in the background a steep-gabled half-timbered house with brick infill
and a brick barn; large bare trees; the road surface cobbles and mud; wooden posts; a piece of field
equipment on the right.
Reproduce: the Brandenburg village mix: 19th-century red-brick buildings with segmental or round-arched
openings (windows about 1.1 x 2.0 m, +/-15 %, against the men), half-timbered barns and houses with
brick nogging, brick walls closing the farmyards to the street (about 2 m high), street trees of 0.5-0.8 m
trunk diameter.
Use: village street grammar; the enemy's route through the village.

**[a2-01-04]** `a2_01_villages_farmsteads/a2-01_volkssturm-carting-grain-through-farm-gateway_barch-183-j28783.jpg`
Bundesarchiv Bild 183-J28783, Herbert Ahrens (SS-PK), issued 13 March 1945, a Rhineland village.
Grade A-. **[eq]** (Rhineland, not the Oder). [prop]
Shows: the view out of a barn gateway: a heavy timber door frame, an iron hoist hook hanging in the
arch, a sack barrow with a grain sack, four men in field caps; outside, a plastered courtyard wall and a
house in bright haze; slush on the ground.
Reproduce: the barn gateway as a dark frame onto a bright yard (a strong interior/exterior light ratio),
hoist hook and sack barrow as working objects. Barn gate 3.0-3.5 m high (+/-20 %).
Use: farmstead interior-to-yard composition; not evidence for Oderbruch architecture.

**[a2-01-05]** `a2_01_villages_farmsteads/a2-01_howitzers-firing-beside-brick-farmstead-east-prussia_1945.jpg`
Emmanuil Evzerikhin, 1945, East Prussia, 3rd Belorussian Front. Grade B. **[eq]** (East Prussia). [prop]
Shows: three 122 mm howitzers firing from bare level ground; behind them a two-storey brick farm building
with its roof stripped to the battens, a gabled brick barn, a three-storey brick block (granary or
manor), a row of bare avenue trees; the muzzle smoke is a bright cloud lit from behind by a low sun;
ammunition boxes, a spoil heap.
Reproduce: a Prussian brick Gehöft beside open fields: two-storey brick buildings 7-9 m to the eaves
(+/-25 %, against the gunners), gable roofs of 40-50 degrees, roofs that lose tiles first and show their
battens; smoke glowing when backlit.
Use: farmstead massing and roof damage; artillery in the village edge.

**[a2-01-06]** `a2_01_villages_farmsteads/a2-01_signallers-by-thatched-barn-gumbinnen_1945-01-13.jpg`
D. Soroka, 13 January 1945, near Gumbinnen, East Prussia (Russian State Museum Catalogue). Grade A-.
**[eq]** [winter].
Shows: signallers running across frozen, rough-ploughed ground past a low shed with a thick thatch that
overhangs almost to the ground; a bare tree; low sun, long shadows; no snow cover.
Reproduce: the humblest farm outbuilding (a thatched shed of about 2.5 m eaves, +/-25 %) at a field edge;
ploughed ground in ridges.
Use: form only; January light and ground.

**[a2-01-07]** `a2_01_villages_farmsteads/a2-01_bread-oven-at-farmhouse-westphalia_iwm-bu3062.jpg`
IWM BU 3062, Sgt Laing (No. 5 AFPU), 4 April 1945, Westphalia. Grade A. **[eq]** (west).
Shows: soldiers eating beside a jeep next to a whitewashed domed brick bread oven in a copse of bare
birches and a conifer; trodden mud and leaf litter.
Reproduce: the free-standing farm bread oven (Backofen): dome about 1.2 m high and 2 m long, arched mouth
about 0.5 m wide (+/-20 %, against the seated men), whitewashed. In the first week of April the birches
and other deciduous trees are still bare.
Use: yard dressing; the early-April state of deciduous trees.

### 2.2 Farmland (`a2_02`, 5)

**[a2-02-01]** `a2_02_farmland/a2-02_kuestrin-fortress-and-oder-lowland-aerial_nara-rg373-221963703.jpg`
NARA Record Group 373, NAID 221963703, vertical aerial photograph, 18 March 1945, Küstrin. Grade A.
**In region.**
Shows: the fortress island (Altstadt) between the Oder and the Warthe with its bastions, the Neustadt's
street grid and rail yards, road and rail bridges, broad dark rivers and flooded meadows; cumulus clouds
with their shadows over the town. Eleven days before the fortress fell.
Reproduce: the northern anchor of the Oderbruch battlefield: river widths of 150-250 m at this date
(+/-25 %, against the street blocks), floodwater on the meadows, a town packed inside its bastions. In
Act 2 terms it is the bridgehead's origin, off the map's north-east.
Use: layout of the Küstrin bridgehead end of the act; cloud-shadow scale on flat ground.

**[a2-02-02]** `a2_02_farmland/a2-02_b4-howitzers-on-open-rutted-field_1945-04.jpg`
Aleksandr Ustinov, April 1945, "on the way to Berlin" (Moscow city archive, CC BY 4.0). Grade B.
**Place unverified** (Berlin operation, April 1945).
Shows: a tracked 203 mm B-4 howitzer on a dead-flat bare field churned into dense track ruts; a second
howitzer and a tractor behind; a low line of pine wood on the horizon; low sun from the right, long
shadows, a clear hazy sky.
Reproduce: April fields before growth: bare earth, flat to the horizon, carrying the imprint of every
tracked vehicle (ruts 0.1-0.3 m deep, track pattern visible); a wood line at 1-2 km reads as a thin dark
band.
Use: the Oder plain's ground surface in April; track marks as physical trace.

**[a2-02-03]** `a2_02_farmland/a2-02_photomap-drained-lowland-heinrichswalde_barch-196-00996.jpg`
Bundesarchiv Bild 196-00996, 1:25,000 Bildplan, September 1944, Heinrichswalde (Elchniederung). Grade A-.
**[eq]** [map].
Shows: a vertical photo mosaic of a drained river lowland: small irregular fields, farmsteads strung
along roads, dark wood blocks, the printed grid.
Reproduce: only the pattern: in a Prussian drained lowland field parcels are 50-200 m wide and
farmsteads line the roads rather than clustering. Check against the Oderbruch Messtischblatt 3452.
Use: field-pattern cross-check.

**[a2-02-04]** `a2_02_farmland/a2-02_photomap-river-meanders-and-field-strips_barch-196-01397.jpg`
Bundesarchiv Bild 196-01397, 1:25,000 Bildplan, September 1944, Luisenberg. Grade A-. **[eq]** [map].
Shows: a winding river with a meadow band through long narrow field strips, a forest block above.
Reproduce: strip fields ending at a meadow band along a meandering watercourse (the Alte Oder in the
Oderbruch is such a band).
Use: field-pattern cross-check.

**[a2-02-05]** `a2_02_farmland/a2-02_soviet-artillery-line-before-berlin_barch-183-e0406-0022-012.jpg`
Bundesarchiv Bild 183-E0406-0022-012, April 1945, "before Berlin" (carried over from the 28 September
set, where it is [10-02]). Grade A-. **Place unverified** (Oder front, April 1945). [prop]
Shows: a line of field guns at 20-30 m intervals on flat ploughed farmland, ammunition boxes stacked by
each gun, slit trenches and spoil in the foreground, wheel ruts; village tree lines on the horizon.
Reproduce: the gun density of 16 April: guns wheel to wheel along field edges, boxes three or four high,
slit trenches; a flat horizon broken only by village tree lines at 2-3 km.
Use: the attacker's side of the plain; physical traces (empty boxes, spoil, ruts) after the barrage.

### 2.3 The Oder, canals and ditches (`a2_03`, 9 + 1 link-only)

**[a2-03-01]** `a2_03_river_canals_ditches/a2-03_sappers-building-oder-pontoon-bridge_1945-04.jpg`
Arkady Shaikhet, April 1945, the Oder (Russian MoD, CC BY 4.0). Grade B. **Place unverified** (crossing point
not stated).
Shows: sappers, some waist-deep, laying a pontoon deck beside a destroyed steel truss road bridge whose
spans have dropped into the river; river mist, a pale sky; figures 60 m away already grey.
Reproduce: blown bridges fail span by span: the truss drops into the river at an angle while the piers
stand; the replacement crossing is built alongside. Mist over the river cuts contrast to about half at
50-80 m (a visual estimate on a print; the participating medium must produce it).
Use: river crossings; demolished bridge form; river mist.

**[a2-03-02]** `a2_03_river_canals_ditches/a2-03_signallers-on-flooded-oder-bank_1945-04-21.jpg`
Vladimir Grebnev, 21 April 1945, the Oder's west bank (52nd Guards Rifle Division). Grade B (sector not
verified). **Place unverified.** [prop]
Shows: two signallers prone on the grassy crest of a dike, one on a field telephone, one aiming a
submachine gun; beyond them a sheet of floodwater with a house roof just above the water and fence posts
standing in it; a low wooded shore far off.
Reproduce: in April 1945 the Oder meadows were flooded: open water up to the eaves of low buildings
(water about 2.5-3 m deep over the ground at that house, +/-30 %), fence lines running into the water;
the dike crest (about 1-2 m above the flood) is the only dry line.
Use: the Oder bank and polder edge; the act's eastern boundary.

**[a2-03-03]** `a2_03_river_canals_ditches/a2-03_is2-and-trucks-crossing-oder-on-timber-bridge_1945-02-01.jpg`
Efim Kopyt (TASS), 1 February 1945 (per Commons), the Oder. Grade B. **Place unverified.** [winter]
Shows: a low timber trestle bridge on driven piles with its deck less than a metre above the water, an
IS-2 and a loaded truck crossing, drifting ice; a flat far bank with bare trees.
Reproduce: the combat bridge: pile bents at 4-6 m spacing (+/-25 %, against the truck), a plank deck
without rails, laid just above water.
Use: crossings in the Küstrin bridgehead; bridge geometry that a heavy tank can use.

**[a2-03-04]** `a2_03_river_canals_ditches/a2-03_volkssturm-mg-position-on-oder-bank_barch-183-j28732.jpg`
Bundesarchiv Bild 183-J28732, Pincornelly (PK), 15 February 1945, the Oder near Frankfurt. Grade A.
**In region.** [prop]
Shows: an MG 42 on a plank table in a bank position lined with timber, a Panzerfaust in the foreground,
a helmet on the parapet; the broad Oder with floating ice, the far bank low.
Reproduce: German bank positions are cut into the dike or bank and revetted with planks; the field of
fire is the open water.
Use: the German side of the river line.

**[a2-03-05]** `a2_03_river_canals_ditches/a2-03_volkssturm-dugout-on-oder-bank_barch-183-j28787.jpg`
Bundesarchiv Bild 183-J28787, Pincornelly (Scherl), 12 February 1945, Frankfurt (Oder). Grade A.
**In region.** [prop]
Shows: a trench cut into the bank with a plank revetment, four men in it, an officer in a leather coat
standing above; across the water the town's silhouette and the multi-arch Oder bridge; on the far bank
trees standing in floodwater.
Reproduce: the Oder at Frankfurt about 200-250 m wide (+/-30 %, against the bridge); floodwater among
the trees of the far bank already in February.
Use: river width and bank profile at the south end of the front.

**[a2-03-06]** `a2_03_river_canals_ditches/a2-03_temporary-timber-bridge-frankfurt-oder_1945.jpg`
Unknown photographer, "ca. 1945", Frankfurt (Oder) (private scan). Grade C. **In region.**
Shows: a long timber pile bridge with raked board ice-breakers upstream of each pier; behind it a
gutted four-storey building, tall factory chimneys, a domed building and burnt-out blocks; a sandy bank.
Reproduce: the post-battle river town: blocks burnt out behind the waterfront, chimneys standing.
Use: form only; the date is not secure.

**[a2-03-07]** `a2_03_river_canals_ditches/a2-03_border-post-on-oder-bank-rubble_1945.jpg`
Unknown photographer, 1945, the Oder bank (scan from a 1980 Polish textbook). Grade C. **Place unverified**
(place not stated). [staged]
Shows: soldiers setting a striped border post into a bank heaped with pale brick and stone rubble, the
flat river and a smoky far shore behind; spring grass.
Reproduce: rubble dumped along river banks (from destroyed buildings); spring grass.
Use: bank dressing; staged ceremony, not an event of the battle.

**[a2-03-08]** `a2_03_river_canals_ditches/a2-03_armoured-river-boat-at-town-quay_1945-04.jpg`
Russian MoD (berlin1945.mil.ru), April 1945, "Grossen" (probably Crossen an der Oder). Grade B.
**[eq]** (the Oder about 70 km upstream of Frankfurt, if "Grossen" is Crossen).
Shows: an armoured river boat with a tank turret passing a timber quay with a broken landing stage;
two-storey stuccoed houses with hipped roofs and bare trees behind.
Reproduce: the river-town edge: quay, landing stage, low stuccoed houses.
Use: river dressing.

**[a2-03-09]** `a2_03_river_canals_ditches/a2-03_patrol-boat-bromberg-canal-blown-bridge_1945-04.jpg`
Russian MoD, April 1945, Bromberg Canal (Poland). Grade B. **[eq]**
Shows: a patrol boat on a canal passing a blown bridge whose deck slants into the water, timber and
earth heaped on the abutment; stuccoed houses and bare trees.
Reproduce: small canal bridges fail by dropping the deck into the channel with the abutments intact; a
drainage-canal bridge in the Oderbruch blown by the defenders would look like this at a smaller scale.
Use: canal bridge demolition.

Link-only O1 (in the manifest): Valery Faminsky, Red Army soldiers in the Seelow Heights area, April-May
1945, a river crossing with timber members (Arthur Bondar's private collection, exhibited at the
Gedenkstätte Seelower Höhen). Not analysed.

### 2.4 The scarp (`a2_04`, 2)

No open-licence photograph shows the Seelow Heights as a landform: its 40-50 m scarp, the Seelow town
on its rim, the Reichsstraße 1 climb, Friedersdorf or Dolgelin. The two photographs here are views
*from* observation posts; the landform must come from the DEM and the Messtischblatt.

**[a2-04-01]** `a2_04_scarp/a2-04_front-command-observation-post-16-april_1945-04-16.jpg`
Anatoly Arkhipov (TASS), 16 April 1945, 8th Guards Army observation post in the Küstrin bridgehead;
Chuikov, Telegin, Kazakov and others. Grade B. **In region.** [prop]
Shows: seven officers behind a pale plank parapet (a board surface with papers pinned to it), one on a
field telephone, one at a binocular scissors periscope; overhead a fishing-net-type camouflage net laid
over pine boughs, with holes; bright hazy daylight. The photographer stands inside the post.
Reproduce: the command post on the Reitwein spur as a dug-in, roofed-over box: plank parapet at chest
height, net and cut pine over poles, the only openings forward. The spur is the one piece of high ground
on the Soviet side from which the Oderbruch and the heights beyond could be seen.
Use: the attacker's view point; the morning of 16 April.

**[a2-04-02]** `a2_04_scarp/a2-04_chuikov-observation-post-seelow_1945-04.jpg`
Russian MoD (mil.ru), April 1945, 8th Guards Army OP in the battles for the Seelow Heights; Chuikov and
A. M. Pronin (carried over from the 28 September set, [10-01]). Grade B. **In region.**
Shows: three officers behind a pale parapet with a scissors periscope and binoculars, a camouflage net
over pine branches overhead, earth walls and a plank frame.
Reproduce: as [a2-04-01]; same post type, probably the same post.
Use: pairs with [a2-04-01].

### 2.5 Damage (`a2_05`, 5)

**[a2-05-01]** `a2_05_damage/a2-05_podelzig-rubble-and-church-ruin_barch-183-n0301-362.jpg`
Bundesarchiv Bild 183-N0301-362, Otto Donath, 1945, Podelzig. Grade A-. **In region.** [prop]
Shows: a house reduced to a low field of broken masonry: plaster-faced brick lumps of 0.2-0.5 m, wall
stubs 0.5-1 m high, one charred timber post about 3 m tall; 100 m beyond, the church's shaped gable
standing alone among stripped trees and telegraph poles.
Reproduce: **a leveled farmhouse is a rubble field about 0.5-1 m deep spread over its own footprint, not
standing walls**; the roof timber has burned; only chimneys, posts and the church gable rise above it.
Use: the destroyed-home state for the act's rendezvous.

**[a2-05-02]** `a2_05_damage/a2-05_podelzig-clearing-work-under-dead-trees_barch-183-n0301-361.jpg`
Bundesarchiv Bild 183-N0301-361, Otto Donath, 1945, Podelzig. Grade A-. **In region.** [prop]
Shows: five villagers (women in aprons and headscarves, men in jackets) with spades and hoes in a
trampled clearing; behind them the church gable ruin and trees stripped to their trunks and main limbs;
a length of hose or pipe on the ground.
Reproduce: shell-stripped trees keep trunk and main limbs but lose all twigs and most branches; they
stand dead for months.
Use: the aftermath; people returning (only after the battle, not in the act's timeline).

**[a2-05-03]** `a2_05_damage/a2-05_podelzig-church-tower-ruin_barch-183-n0816-315.jpg`
Bundesarchiv Bild 183-N0816-315, Otto Donath, 1945, Podelzig. Grade A-. **In region.** [prop]
Shows: the church tower as a gutted shaft of rubble masonry with a ragged top and iron beam ends
sticking out, a round opening, and at its foot a small gabled porch with a pointed-arch doorway and an
inscription plaque; shrubs against the wall.
Reproduce: village church towers survive as hollow shafts after the roof and bell frame burn; the
porch and door frame last longest. Tower about 14-16 m to the broken top (+/-25 %, against the door).
Use: the village church landmark in its destroyed state (`Village Church` in `world.rs`).

**[a2-05-04]** `a2_05_damage/a2-05_reitwein-station-ruin_barch-183-n0816-311.jpg`
Bundesarchiv Bild 183-N0816-311, Otto Donath, 1945, Reitwein station. Grade A-. **In region.** [prop]
Shows: a single-storey brick station building about 20 m long with its roof burnt to a few rafters and
sheets, three chimney stacks standing, window openings blown out, one wall section collapsed; the brick
platform edge; a track bed with sleepers and ballast but no rails.
Reproduce: railway buildings on the Küstrin-Frankfurt line at the foot of the Reitwein spur: brick,
single storey, burnt out; missing rails (the caption does not say why; do not invent a cause).
Use: the railway at the scarp's foot.

**[a2-05-05]** `a2_05_damage/a2-05_burning-inn-ruin-village-fight-east-prussia_1945-02-20.jpg`
Leonid Dorensky, 20 February 1945, a locality south-east of Königsberg. Grade A-. **[eq]** [prop]
Shows: soldiers running across a rubble field below a gutted two-storey brick building with 'Hotel
Schloss' painted on its render band; black smoke pours from a window; a bent iron bedstead in the
rubble; patches of snow.
Reproduce: a village inn at the moment of burning: black smoke from windows (burning contents, not
walls), rubble spread 5-10 m into the street, domestic iron (bedsteads) surviving the fire. The painted
sign is readable text and must not be legible in game.
Use: burning-state reference; domestic debris.

### 2.6 Vehicles and wrecks (`a2_06`, 7)

**[a2-06-01]** `a2_06_vehicles_wrecks/a2-06_t34-85-with-riders-on-shelled-avenue_1945-04.jpg`
Unknown photographer, April 1945, 2nd Guards Tank Army in Germany (tank-history website). Grade C.
**Place unverified** (the army attacked from the Küstrin bridgehead; place not stated).
Shows: two T-34-85s crowded with infantry riders on a road lined with big old trees whose crowns have
been shot off to stubs; an open field beyond.
Reproduce: the shelled avenue (Allee): trunks 0.6-1.0 m thick at 8-12 m spacing (+/-25 %, against the
tank), crowns reduced to stubs; riders on every tank.
Use: Soviet armour on the Oderbruch roads; common-enemy T-34 presentation.

**[a2-06-02]** `a2_06_vehicles_wrecks/a2-06_is2-on-shelled-approach-road_1945.jpg`
Yevgeny Khaldei, 1945, "on the approaches to Berlin" (russiainphoto.ru 184803). Grade B. **[eq]**
(Berlin's outer edge, place not stated).
Shows: IS-2 '434' and a second IS-2 on a road whose paving has been torn up and heaped at the kerb, bare
shell-stripped trees and lamp posts along it, men in the road; buildings of four or five storeys dissolve
into smoke haze at 150-300 m.
Reproduce: heavy traffic and shelling tear up cobbles into loose heaps; smoke haze removes the
background within a few hundred metres.
Use: IS-2 form; road damage; haze.

**[a2-06-03]** `a2_06_vehicles_wrecks/a2-06_isu-assault-gun-beside-prisoner-column_1945-05.jpg`
Yevgeny Khaldei, May 1945, a German town. Grade B. **[eq]**
Shows: an ISU heavy assault gun with a thick short gun (ISU-152 type) in a street of four- and
five-storey houses with bay windows, a column of German prisoners with packs and mess tins walking past.
Reproduce: the Act 2 boss's class: a low casemate on the IS chassis, gun 2.5-3 m proud of the hull front,
people shoulder-high to the track guards.
Use: the ISU-152 boss silhouette against people and buildings.

**[a2-06-04]** `a2_06_vehicles_wrecks/a2-06_is2-column-on-tree-lined-road_1945-01.jpg`
Efim Kopyt (TASS), January 1945, "East Prussia, 1st Belorussian Front" (inconsistent). Grade B-.
**[eq]**
Shows: IS-2s nose to tail under bare avenue trees past a large two-storey stuccoed building with a hipped
tile roof; a camouflaged German half-track on the right; the road littered with debris including
abandoned Panzerfaust tubes.
Reproduce: a Soviet heavy tank column moves nose to tail (2-5 m gaps) on the paved road; the verges carry
what the defenders dropped.
Use: column formation; roadside debris.

**[a2-06-05]** `a2_06_vehicles_wrecks/a2-06_t34-85-column-in-avenue-of-trees_1945-03-01.jpg`
Boris Velyashev, 1 March 1945, East Prussia. Grade B. **[eq]** [prop]
Shows: a long T-34-85 column (tank '221') with riders under a double row of old bare avenue trees on a
cobbled road; slogans painted on the turrets.
Reproduce: the tree-lined cobbled road as a corridor; painted slogans are text and must not be legible.
Use: road and column.

**[a2-06-06]** `a2_06_vehicles_wrecks/a2-06_damaged-tiger2-on-flat-farmland_nara-111-sc-197514.jpg`
NARA 111-SC-197514, 28 November 1944, Freialdenhoven (Rhineland). Grade A. **[eq]**
Shows: a Tiger II in open flat farmland with one track loose, hit marks on the turret side, a GI on the
turret and an officer crouching by the hull; a flat horizon with a telegraph pole.
Reproduce: a knocked-out heavy tank in an open field: it sits where it stopped, track run off, no crater
round it; the only verticals for hundreds of metres are poles.
Use: wreck in the open field; scale of the Tiger II.

**[a2-06-07]** `a2_06_vehicles_wrecks/a2-06_tiger2-lifted-by-wrecker-crane_1944-12-20.jpg`
Pvt. William C. Sanderson (US Army), 20 December 1944, Western Front. Grade A-. **[eq]**
Shows: a heavy wrecker's jib with chain and hook working on a Tiger II's turret in a muddy flat field;
sepia print.
Reproduce: the mass problem: moving a Tiger II turret needs a crane truck.
Use: recovery; not an Act 2 event.

### 2.7 People and refugees (`a2_07`, 10 + 1 link-only)

Most of the Oderbruch's civilians had been evacuated from February 1945 onwards (standard accounts; not verified in this set); the refugees who
crossed it came from further east. None of these photographs is from the Oderbruch. They set dress, loads
and vehicles for the traces a player finds, not people present in the act (the act is empty of the living
at the rendezvous).

**[a2-07-01]** `a2_07_people_refugees/a2-07_refugees-on-foot-with-bundles_barch-146-1985-021-09.jpg`
Bundesarchiv Bild 146-1985-021-09, 1945, place not stated ("refugees move westward"). Grade A-. **[eq]**
Shows: women and men walking away along a road beside a clipped hedge and bare trees, each carrying a
large bundle, a suitcase or a bedding roll on the back; one pulls a sack on a small two-wheeled cart.
Reproduce: what people carried when they walked: bedding tied in sheets, cardboard suitcases, a
handcart. Left behind on a road, these objects are the trace of a trek.
Use: dressing (dropped bundles, suitcases, handcarts).

**[a2-07-02]** `a2_07_people_refugees/a2-07_refugee-wagons-massed-east-prussia_barch-146-1979-084-05.jpg`
Bundesarchiv Bild 146-1979-084-05, March 1945, East Prussia. Grade A-. **[eq]** [winter]
Shows: hundreds of covered and open farm wagons and horses packed on a slope above a hazy frozen shore.
Reproduce: a halted trek is a dense park of wagons 1-3 m apart with canvas tilts on hoops.
Use: the scale of a trek; wagon forms.

**[a2-07-03]** `a2_07_people_refugees/a2-07_refugee-wagon-on-cobbled-street_barch-146-1996-028-36a.jpg`
Bundesarchiv Bild 146-1996-028-36A, Brigitte Höber, 20-21 February 1945, Danzig area. Grade A. **[eq]**
Shows: a flat farm wagon (plank sides, spoked wheels) with two horses, loaded with bedding, baskets, a
bucket and a milk can, passing three-storey town houses on a cobbled street with tram rails; rubble heaps
on the pavement.
Reproduce: the Leiterwagen/Kastenwagen as loaded: 4-4.5 m bed, wheels about 1 m, goods tied on top.
Use: wagon and load reference.

**[a2-07-04]** `a2_07_people_refugees/a2-07_refugee-horse-wagons-village-edge_barch-175-s00-00326.jpg`
Bundesarchiv Bild 175-S00-00326, 1945, East Prussian trek. Grade A-. **[eq]**
Shows: horse-drawn ladder wagons full of women and children under blankets at a village edge; behind them
a burnt house with only its chimney standing, pines and a birch; a wooden box in the foreground.
Reproduce: burnt farmhouses leave the chimney stack standing alone; wagons stop beside them.
Use: village-edge composition; chimney-only ruin form.

**[a2-07-05]** `a2_07_people_refugees/a2-07_refugee-woman-with-small-cart_barch-146-1996-036-01.jpg`
Bundesarchiv Bild 146-1996-036-01, 1945, place not stated. Grade A-. **[eq]**
Shows: an elderly woman in a headscarf weeping on a bench by iron railings, holding the handle of a small
child's cart with two tied sacks.
Reproduce: one person's possessions at true scale: a toy-sized cart, two sacks.
Use: an object trace for an abandoned place (a small cart left by a bench says more than a body would).

**[a2-07-06]** `a2_07_people_refugees/a2-07_refugee-wagon-in-town-street_barch-146-1979-084-06.jpg`
Bundesarchiv Bild 146-1979-084-06, 1945, place not stated. Grade A-. **[eq]** [winter]
Shows: women and a man in fur caps on a wagon packed with bedding and sacks, passing a rendered town house.
Use: clothing and bundling of winter treks.

**[a2-07-07]** `a2_07_people_refugees/a2-07_volkssturm-woman-panzerfaust-instruction_barch-146-1973-001-30.jpg`
Bundesarchiv Bild 146-1973-001-30, March 1945, place not stated. Grade A-. **[eq]** [prop]
Shows: a soldier in a slit trench at a field edge showing a woman in a hat and fur collar how to hold a
Panzerfaust; water or a flooded meadow behind.
Reproduce: slit trenches at field edges and the Panzerfaust's form (a tube with a bulbous warhead
about 0.15 m across).
Use: weapon and trench form.

**[a2-07-08]** `a2_07_people_refugees/a2-07_soldiers-with-panzerfausts-feb-1945_barch-183-h28150.jpg`
Bundesarchiv Bild 183-H28150, February 1945, "the front on Germany's eastern border". Grade A-. **[eq]** [prop]
Shows: young and middle-aged soldiers in field caps and scarves, some in snow smocks, each with one or
two Panzerfausts over the shoulder.
Use: the defenders the Soviet tanks met in the villages; discarded Panzerfaust tubes are a common trace
(see [a2-06-04]).

**[a2-07-09]** `a2_07_people_refugees/a2-07_hitler-youth-volkssturm-company_barch-183-j28536.jpg`
Bundesarchiv Bild 183-J28536, February 1945, Pyritz (Pomerania). Grade A-. **[eq]** [prop] (ADN wording
flagged as biased).
Shows: boys in oversized steel helmets and greatcoats with rifles, around an adult leader in a leather
coat, a decorated soldier beside them, in a doorway of a plastered building.
Reproduce: the age of some defenders; oversized equipment.
Use: dress for any trace of child soldiers (an oversized helmet, a small coat) - restraint required.

**[a2-07-10]** `a2_07_people_refugees/a2-07_halftrack-with-soldiers-and-civilians-retreat_barch-183-h26408.jpg`
Bundesarchiv Bild 183-H26408, 2 February 1945, Breslau. Grade A. **[eq]** [prop]
Shows: a whitewashed Sd.Kfz. 11 half-track overloaded with soldiers, bundles and a civilian, towing a
gun under foliage; a timber shed, gabled houses and a bare tree behind.
Reproduce: German vehicles in the last months carried people and bundles on every surface.
Use: German vehicle loading; whitewash wearing off by February.

Link-only O2: Valery Faminsky, transport of wounded with dog teams near the Seelow Heights, April 1945
(private collection). Not analysed.

### 2.8 Spring weather and light (`a2_08`, 3)

**[a2-08-01]** `a2_08_spring_weather_light/a2-08_katyusha-salvo-at-night_1945.jpg`
P. Zhukovsky (TASS), before February 1945, 1st Belorussian Front. Grade B. **[eq]** [prop] [print]
(possibly retouched).
Shows: rockets leaving straight bright streaks across a black sky; the launch smoke lit from below as a
bright billowing cloud; a two-storey house and roofs as black silhouettes; snow on the ground.
Reproduce: in a night barrage the only light is the weapons': streaks and muzzle flashes light the smoke
from inside, and the smoke then glows and scatters that light onto the ground. The Seelow attack began at
03:00 on 16 April under this kind of light plus the searchlights (no photograph of the searchlights was
found).
Use: night-barrage light; smoke as a lit participating medium.

**[a2-08-02]** `a2_08_spring_weather_light/a2-08_carriers-through-burning-village-smoke_iwm-bu3053.jpg`
IWM BU 3053, Sgt Norris (No. 5 AFPU), 4 April 1945, Levern (Westphalia). Grade A. **[eq]**
Shows: a carrier driving along a village street through smoke from burning farm buildings: a
half-timbered frame standing open, a tall chimney, telegraph pole and bare trees; the smoke turns
everything beyond 30-60 m into flat grey.
Reproduce: smoke from a burning village is dense and low: contrast falls to near zero within about 50 m
(visual estimate); vertical structures (chimneys, poles) are the last to disappear.
Use: burning-village atmosphere in April.

**[a2-08-03]** `a2_08_spring_weather_light/a2-08_anti-aircraft-salvo-april-sky_1945-04-04.jpg`
Vyacheslav Sharovsky (TASS), 4 April 1945, East Prussia. Grade A-. **[eq]** [prop]
Shows: an 85 mm anti-aircraft gun firing from a circular pit with a straw-covered earth parapet; dark shell
bursts in a pale, hazy April sky; an officer standing on flat grassland; a wood line on the horizon.
Reproduce: early-April grassland (dry, pale, short), a hazy bright sky, bursts as dark smoke puffs.
Use: open-country April light; gun-pit construction.

## 3. Catalogue, Act 3: Halbe and the Kessel von Halbe

Only one downloaded photograph shows the pocket ([a3-05-01], weak provenance) and one link-only record
shows Forsthaus Halbe after the fighting. Everything else is a labelled equivalent chosen for one
property the act needs: the sandy pine forest, the small brick-and-render town, the Reichsautobahn, the
Tiger II, and people on forest roads. The equivalents come from three kinds of place, which must not be
mixed up in a render: the north German pine heath and the Reichswald (forest), Pomeranian and West
Prussian small towns (townscape), and the Reichsautobahn in northern and western Germany (motorway).

### 3.1 Pine forest (`a3_01`, 10 + 1 link-only)

**[a3-01-01]** `a3_01_forest/a3-01_abandoned-rocket-halftracks-in-pine-forest_iwm-bu3430.jpg`
IWM BU 3430, Sgt Laing (No. 5 AFPU), 13 April 1945, forest near Celle. Grade A. **[eq]**
Shows: two Panzerwerfer half-tracks and several rocket-launcher trailers abandoned under open,
crooked, high-crowned heath pines with a few deciduous trees; a flat floor of leaf and needle litter; a
sandy track curving through; a jerrycan-sized box on the ground.
Reproduce: the sandy heath pine forest in mid-April: pines 15-20 m high (+/-25 %, against the
half-track's 2.5 m), crowns only in the top third, trunks often curved, spacing 4-8 m, no shrub layer;
vehicles pulled off the track under the trees and left.
Use: the forest south of Halbe as the breakout's ground; abandoned equipment in place.

**[a3-01-02]** `a3_01_forest/a3-01_carriers-on-pine-forest-road_iwm-b14458.jpg`
IWM B 14458, Sgt Silverside (No. 5 AFPU), 10 February 1945, Reichswald. Grade A. **[eq]** [winter]
Shows: carriers on a straight unpaved forest road between tall, straight pines bare of branches for most
of their height; brush, slash and dead bracken along the verges; vehicles at 100-200 m grey in haze.
Reproduce: the forest road (Gestell) as a straight corridor 6-10 m wide between walls of trunks; the
view down it ends in haze, not at a horizon.
Use: forest road layout and depth.

**[a3-01-03]** `a3_01_forest/a3-01_infantry-and-tanks-on-pine-forest-road_iwm-b14454.jpg`
IWM B 14454, Sgt Silverside, 10 February 1945, Reichswald. Grade A. **[eq]** [winter]
Shows: infantry walking beside Churchill tanks on a rutted forest road, dense younger pines on the left,
taller pines on the right, the sandy surface churned into deep ruts.
Reproduce: a sand road under tracked traffic becomes a pair of ruts 0.2-0.4 m deep with a raised
crown; stands of different ages meet at the road.
Use: road surface after the breakout columns.

**[a3-01-04]** `a3_01_forest/a3-01_infantry-file-through-pine-stand_iwm-bu1749.jpg`
IWM BU 1749, Sgt Wilkes, 8 February 1945, Reichswald. Grade A. **[eq]** [winter]
Shows: a file of infantry inside an even-aged pine stand: straight trunks about 0.2-0.3 m thick at 2-4 m
spacing, crowns only at the top, dead grass and needles on the floor, a shallow ditch in front.
Reproduce: inside a managed stand the sight line between trunks is 30-80 m; the floor is flat and open.
Use: fighting and visibility inside the stand.

**[a3-01-05]** `a3_01_forest/a3-01_soldier-in-former-german-trench-forest_iwm-b14445.jpg`
IWM B 14445, No. 5 AFPU, 11 February 1945, Reichswald. Grade A. **[eq]** [winter]
Shows: a soldier in a trench with a heaped sandy parapet, a groundsheet over it and a Bren gun on top.
Reproduce: forest fighting positions: trench spoil heaped 0.5-0.8 m high, sandy and pale where fresh.
Use: dug positions and their spoil.

**[a3-01-06]** `a3_01_forest/a3-01_infantry-in-forest-undergrowth_iwm-b14413.jpg`
IWM B 14413, Sgt Silverside, 9 February 1945, Reichswald. Grade A. **[eq]** [winter]
Shows: soldiers with a Bren gun among cut pine branches and young pines at a clearing edge, tall thin
trees behind.
Reproduce: slash and young growth at clearings cut sight to a few metres.
Use: close cover at forest edges.

**[a3-01-07]** `a3_01_forest/a3-01_tanks-halted-on-narrow-forest-track_iwm-bu1751.jpg`
IWM BU 1751, Sgt Wilkes, 8 February 1945, Reichswald. Grade A. **[eq]** [winter]
Shows: Churchill tanks and carriers nose to tail on a track one vehicle wide, a steep bank on the left,
tall bare deciduous trees (beech or oak), mist.
Reproduce: a column on a forest track cannot pass or turn; a single disabled vehicle blocks it.
Use: the breakout column's trap (the Halbe columns jammed exactly so); not a pine reference.

**[a3-01-08]** `a3_01_forest/a3-01_t34-85-column-in-pine-forest_1945.jpg`
Unknown Soviet photographer, 1945, 3rd Guards Tank Army (book scan). Grade C. **[eq]**
Shows: T-34-85s crowded with riders on a narrow road through tall, dense pines, trunks bare to about
10 m, the verges sandy with patches of snow or pale sand.
Reproduce: Soviet armour in pine forest; tanks fill the road.
Use: the enemy in the Halbe forest; form only.

**[a3-01-09]** `a3_01_forest/a3-01_trench-dug-in-sand-under-pines_barch-183-j30355.jpg`
Bundesarchiv Bild 183-J30355, Tomaschek, August 1944, East Prussia. Grade A-. **[eq]** [prop]
Shows: a deep, narrow, steep-sided trench cut through pale sand under pines, spoil along both lips,
bare-chested labourers; bright sun and hard shadows.
Reproduce: the soil under a Brandenburg pine forest is pale sand from a few centimetres down: every
trench, crater and track cut shows pale sand against the dark litter.
Use: soil profile for craters, trenches and ruts.

**[a3-01-10]** `a3_01_forest/a3-01_photomap-forest-block-with-rides_barch-196-01294.jpg`
Bundesarchiv Bild 196-01294, 1:25,000 Bildplan, August 1944, Schirrau. Grade A-. **[eq]** [map]
Shows: a large dark forest block crossed by straight rides and dotted with clearings, farmland lapping
its southern edge.
Reproduce: managed Prussian forests are divided by a rectangular grid of rides (Gestelle) into numbered
compartments (Jagen); check the Halbe forest's grid against Messtischblatt 3848.
Use: forest layout.

Link-only H1: Forsthaus Halbe after the fighting, Red Army photograph (Archiv Uwe Klar): the forester's
lodge beside tall straight trunks, vehicles squeezed into the approach, cut and fallen timber in the
foreground. Not downloaded; not analysed beyond its caption and the 28 September board note.

### 3.2 The small town (`a3_02`, 10)

Halbe in 1945 was a small railway village-town (two-storey houses in plots along a few streets, a
church, the station, villa-type houses at the forest edge). No wartime photograph of it was found. The
equivalents are Pomeranian and West Prussian towns (brick and render, Protestant North German
townscape), the Lower Rhine towns of Kleve and Goch (brick, war-damaged), and Berlin's southern villa
suburbs, taken by the army that also closed the Halbe pocket.

**[a3-02-01]** `a3_02_town/a3-02_t34-85s-in-suburban-villa-street_1945-04.jpg`
Unknown photographer, April 1945, 7th Guards Tank Corps (3rd Guards Tank Army) south of Berlin. Grade C.
**[eq]**
Shows: a T-34-85 and an IS-2 in a suburban street of villas and gardens, tall pines and birches in first
leaf, a cast-iron lamp standard, low garden walls, mist.
Reproduce: the Brandenburg villa-colony edge where the town meets the pines: detached two-storey houses
with steep roofs set back in gardens, pines left standing in the plots; birches just leafing in late April.
Use: the town's forest edge; late-April leaf state.

**[a3-02-02]** `a3_02_town/a3-02_is2-in-small-town-street-stargard_1945-03-19.jpg`
Unknown photographer, 19 March 1945, Stargard (Pomerania) (Stargard museum). Grade B. **[eq]**
Shows: an IS-2 with men on the hull in a street of plain three- and four-storey rendered houses; snow
patches in the gutter; a second vehicle behind.
Reproduce: a heavy tank almost fills a small-town street of 8-10 m between facades (+/-20 %, against the
tank's 3.1 m width).
Use: tank-to-street scale.

**[a3-02-03]** `a3_02_town/a3-02_street-fight-in-pomeranian-town_1945.jpg`
Boris Velyashev, 1945, a Pomeranian locality. Grade B. **[eq]** [prop] [winter]
Shows: Soviet soldiers firing submachine guns from a doorway and along a street of brick and rendered
houses, a painted shop sign on the wall ("Eigene Backwaren Fabrik"), lath and plaster fallen from a
wall, broken timber, snow.
Reproduce: plaster ceilings and lath fall first; doorways are the fighting positions. The painted sign
is readable text and must not be legible in game.
Use: close fighting in a small town.

**[a3-02-04]** `a3_02_town/a3-02_cavalry-in-town-street-neustettin_1945.jpg`
Efim Kopyt, 1945, Neustettin (Pomerania). Grade B. **[eq]** [prop]
Shows: cavalry with a banner riding along a street of undamaged two- and three-storey rendered houses
with pedimented window heads and dormers; a shop sign.
Reproduce: the intact small-town frontage before damage: continuous rendered facades, windows about
1.0 x 1.8 m on 2.5-3 m axes (+/-20 %, against the riders).
Use: townhouse grammar.

**[a3-02-05]** `a3_02_town/a3-02_armoured-car-before-brick-town-hall_1945-03-27.jpg`
Boris Velyashev, 27 March 1945, Stolp (Pomerania). Grade A-. **[eq]**
Shows: a BA-64 armoured car on a cobbled square before a large brick Gothic-revival town hall with a
clock tower, stepped gables and pointed windows; bare trees.
Reproduce: the North German brick public building; cobbled squares.
Use: brick public building type (Halbe itself had no such building; use for the church and station
brickwork only).

**[a3-02-06]** `a3_02_town/a3-02_street-barricade-of-doors-wollin_barch-183-h26409.jpg`
Bundesarchiv Bild 183-H26409, 17 March 1945, Wollin (Pomerania). Grade A. **[eq]** [prop]
Shows: two German soldiers running across a junction past a barricade made of doors, boards and a
corrugated sheet propped against a rendered house; a stone bollard; shutters.
Reproduce: improvised barricades from household material, 2-2.5 m high, in front of house walls. At
Halbe the defenders' and the breakout's barricades were built from what the town held (the Halbe page's
modern captions mention a barricade on Lindenstraße; its form is not photographed).
Use: barricade form.

**[a3-02-07]** `a3_02_town/a3-02_tanks-and-infantry-in-ruined-town-kleve_iwm-b14611.jpg`
IWM B 14611, Sgt Wilkes, 11 February 1945, Kleve. Grade A. **[eq]** [winter]
Shows: a Churchill tank and infantry on a road through a shelled town edge: roofs burnt to their frames,
a church-like ruin, rubble spilling from the plots onto both verges, a leafless tree.
Reproduce: rubble lies in plots and against facades, leaving a cleared lane for traffic.
Use: town damage from shelling.

**[a3-02-08]** `a3_02_town/a3-02_tanks-in-rubble-street-goch_iwm-b14779.jpg`
IWM B 14779, Sgt Christie, 21 February 1945, Goch. Grade A. **[eq]** [winter]
Shows: two tanks in a street of three-storey brick houses with windows blown out and fronts pocked; the
roadway covered with brick and tile rubble; mist at the end of the street.
Reproduce: a street after fighting: continuous rubble layer 0.1-0.5 m on the roadway, facades standing.
Use: street rubble state.

**[a3-02-09]** `a3_02_town/a3-02_t34-85-at-railway-crossing-in-town_1945-05.jpg`
Unknown photographer, May 1945, Dresden area (per Commons). Grade C. **[eq]**
Shows: a T-34-85 with riders crossing tracks beside a railway signal gantry, a steam locomotive, soldiers
on the ballast, a church spire and brick buildings behind, smoke.
Reproduce: the railway through a town (Halbe had its station and level crossing): semaphore signals,
ballast, a locomotive as a large obstacle.
Use: station and level-crossing dressing.

**[a3-02-10]** `a3_02_town/a3-02_street-fight-in-ruins-wehlau_1945.jpg`
Dmitry Chernov, January-February 1945, Wehlau (East Prussia). Grade B. **[eq]** [prop] [winter]
Shows: two soldiers firing from behind a rubble heap in a street of gutted brick houses with empty window
rows; smoke.
Reproduce: rubble heaps as tall as a crouching man (about 1-1.2 m) are the cover in a ruined street.
Use: ruin cover.

### 3.3 The Autobahn and roads (`a3_03`, 9)

The Halbe breakout had to cross the Reichsautobahn Berlin-Dresden (today's A13), where Soviet blocking forces
waited. All Reichsautobahn sections were built to one standard; the photographs below are from other
sections and show the standard.

**[a3-03-01]** `a3_03_autobahn_roads/a3-03_tank-guarding-autobahn-underpass_iwm-bu4157.jpg`
IWM BU 4157, Sgt Johnson, 20 April 1945, Bremen-Hamburg autobahn. Grade A. **[eq]**
Shows: two soldiers with a Bren gun behind a grassy mound under a shrub in fresh leaf; a camouflaged
Sherman Firefly beside the carriageway; a concrete overbridge; the carriageway's sandy verge.
Reproduce: the motorway blocking position: guns and tanks at an overbridge, infantry dug into the verge;
by 20 April shrubs are in fresh leaf.
Use: the Soviet blocking line on the A13 (in form).

**[a3-03-02]** `a3_03_autobahn_roads/a3-03_tank-on-autobahn-by-distance-sign_iwm-bu4166.jpg`
IWM BU 4166, Sgt Smith, 20 April 1945, Bremen-Hamburg autobahn. Grade A. **[eq]**
Shows: a Cromwell on a concrete carriageway, a grass verge with a drainage edge, a large white distance
board ("38 km Hamburg"), a concrete overbridge in the distance, open level country, cumulus.
Reproduce: the carriageway of concrete slabs (joints visible), verge 2-3 m, overbridges every few km,
boards at the verge; readable text must not be legible in game.
Use: motorway cross-section and furniture.

**[a3-03-03]** `a3_03_autobahn_roads/a3-03_tank-in-stream-after-slip-road-bridge-collapse_iwm-bu4162.jpg`
IWM BU 4162, Sgt Johnson, 20 April 1945, a slip road to the Bremen-Hamburg autobahn. Grade A. **[eq]**
Shows: a Sherman nose-down in a stream among the broken timber of a small bridge; recovery vehicles on
the bank above; sandy bank.
Reproduce: minor timber bridges give way under a 30-ton tank; a Tiger or IS-2 is heavier still.
Use: bridge failure as a physical outcome, not a scripted one.

**[a3-03-04]** `a3_03_autobahn_roads/a3-03_light-tanks-on-autobahn-overpass_iwm-bu4972.jpg`
IWM BU 4972, Sgt Morris, 2 May 1945, autobahn towards Lübeck. Grade A. **[eq]**
Shows: a Stuart on a wet concrete carriageway with a paved (sett) outer strip, a fence and young trees
in leaf, a concrete overbridge spanning both carriageways, a second vehicle beyond.
Reproduce: overbridge span and clearance (about 4.5-5 m, +/-20 %, against the tank); wet concrete
reflects the sky.
Use: overbridge geometry; wet-surface look.

**[a3-03-05]** `a3_03_autobahn_roads/a3-03_tank-column-on-autobahn-junction_1945-03-31.jpg`
Sgt 1st Class Luke Graziani (US Army), 31 March 1945, near Frankfurt am Main (DVIDS 2346957). Grade A.
**[eq]**
Shows: Sherman tanks on a motorway, a junction sign, a kilometre stone, a helmeted German prisoner in the
foreground, aircraft overhead, the carriageway straight to the horizon.
Use: motorway furniture; column spacing.

**[a3-03-06]** `a3_03_autobahn_roads/a3-03_infantry-dug-in-on-autobahn-embankment_nara-111-sc-270651.jpg`
NARA 111-SC-270651, T/5 McCroby, 30 April 1945, Munich area. Grade A. **[eq]**
Shows: foxholes cut into the grass slope of a motorway embankment about 4-5 m high, a fence along the
crest, a young birch, soldiers moving along the foot of the slope; flat land beyond.
Reproduce: the embankment as a ready-made rampart for a blocking line: foxholes in the slope face,
spoil thrown down the slope.
Use: the A13 blocking line in form; where the breakout met it.

**[a3-03-07]** `a3_03_autobahn_roads/a3-03_blown-autobahn-bridge-aerial_nara-342-fh-3a20645.jpg`
NARA 342-FH-3A20645 (USAAF), about May 1945, Lahntal viaduct. Grade A-. **[eq]**
Shows: an arched stone viaduct with a gap where several arches collapsed into the river, damming it.
Use: demolition form; not the A13 (the Halbe section crosses flat ground without such viaducts).

**[a3-03-08]** `a3_03_autobahn_roads/a3-03_aircraft-hidden-at-autobahn-forest-edge_1945.jpg`
USSTAF photoprint, 1945, a Reichsautobahn section (Library of Congress). Grade B. **[eq]**
Shows: a long straight concrete carriageway with a dense conifer edge right at the verge, telegraph poles,
aircraft pushed under the trees.
Reproduce: where the motorway cuts forest, the tree wall stands 10-20 m from the carriageway edge.
Use: motorway through forest, as at Halbe.

**[a3-03-09]** `a3_03_autobahn_roads/a3-03_tanks-and-cart-on-sandy-road-through-pines_1945.jpg`
Unknown photographer (Polish Army), 1945, on the way to Berlin (NAC). Grade B. **[eq]**
Shows: T-34-85s, a covered horse cart, a man with a bicycle and marching soldiers on a wet road with a
sandy cutting bank; young pines on the slope, pine wood beyond.
Reproduce: the Brandenburg roadside: sand cuttings with young pine, carts and tanks mixed on one road.
Use: road and roadside in the Mark.

### 3.4 Damage (`a3_04`, 4)

**[a3-04-01]** `a3_04_damage/a3-04_tiger2-in-wrecked-street_nara-111-sc-198115.jpg`
NARA 111-SC-198115, T/3 V. C. Calvano, 21 December 1944, Stavelot (Belgium). Grade A. **[eq]** [winter]
Shows: a Tiger II stopped in a narrow street of two-storey rendered houses whose fronts are torn open,
balconies and shutters hanging, timber and slate debris on the road; two soldiers for scale.
Reproduce: a Tiger II fills a village street (street 7-9 m, +/-20 %); facades open to show floors.
Use: the Tiger in Halbe's streets.

**[a3-04-02]** `a3_04_damage/a3-04_tiger2-crashed-into-house_1944-12-18.jpg`
German military photograph (film captured by the US Army), 18 December 1944, Stavelot. Grade B. **[eq]**
Shows: a Tiger II with its rear pushed into the collapsed front of a brick house, rubble heaped against
the hull and tracks.
Reproduce: a heavy tank backing into a masonry house breaks through the wall and buries its engine deck:
the house loses its lower front, the tank is stuck.
Use: tank-building collision outcome (to be produced by simulation, not scripted).

**[a3-04-03]** `a3_04_damage/a3-04_burnt-tiger-at-town-edge-smoke_nara-111-sc-201534.jpg`
NARA 111-SC-201534, 26 February 1945, Rheindahlen. Grade A. **[eq]** [winter]
Shows: smoke drifting across a town edge with a shattered two-storey house; in the foreground a tree
trunk and the burnt tank lying low with its gun horizontal; rubble and a broken pole.
Use: a burnt tank as a road block at a town edge.

**[a3-04-04]** `a3_04_damage/a3-04_t34-85-by-burnt-out-town-houses_1945.jpg`
Emmanuil Evzerikhin, February-April 1945, Brodnica. Grade B. **[eq]**
Shows: a T-34-85 on a bridge approach before a row of burnt-out three-storey houses with empty window rows
and ragged stepped gables; a medieval brick tower on the left; wires overhead; low sun.
Reproduce: burnt-out houses keep their gables and front walls as silhouettes against the sky.
Use: small-town burnt block silhouette.

### 3.5 Vehicles and wrecks (`a3_05`, 7)

**[a3-05-01]** `a3_05_vehicles_wrecks/a3-05_wrecked-german-convoy-in-pine-forest-halbe_1945-04.jpg`
Unknown photographer, late April 1945 (per the uploader), "near the Spreewald during the Battle of Halbe";
Commons source is the Liberation Route Europe website. Grade C (weak provenance). **In region.**
Shows: burnt-out and wrecked vehicles on a sandy forest road under tall straight pines: a burnt van or
ambulance body in the left foreground, an Opel car with its bonnet open, a small truck and field cars, a
line of further wrecks receding into the trees; the ground littered with sheet metal, cloth and debris.
Reproduce: the breakout's end on a forest road: civilian and military vehicles mixed, pushed together,
burnt in place, their contents spread around them; the pines stand undamaged above.
Use: **the** Act 3 reference for the forest road after the breakout; handle its provenance with care.

**[a3-05-02]** `a3_05_vehicles_wrecks/a3-05_tiger2-side-view-on-cobbles_barch-101i-680-8282a-06.jpg`
Bundesarchiv Bild 101I-680-8282A-06, Faupel, October 1944, Budapest (s.Pz.Abt. 503). Grade A. **[eq]**
Shows: a full side view of a Tiger II (production turret, number on the turret side, spare track links,
Zimmerit, soft-edged three-tone camouflage) on cobbles.
Reproduce: the Tiger II silhouette and finish of late 1944 (s.SS-Pz.Abt. 502 at Halbe drove the same
type; its markings are not photographed).
Use: the comrades' tanks.

**[a3-05-03]** `a3_05_vehicles_wrecks/a3-05_tiger2-crew-and-zimmerit-turret-front_barch-101i-680-8282a-09.jpg`
Bundesarchiv Bild 101I-680-8282A-09, Faupel, October 1944, Budapest. Grade A. **[eq]**
Shows: the turret front, mantlet and 8.8 cm barrel close up, ridged Zimmerit on the turret, track links
on the side, two crewmen with headsets in black Panzer uniform.
Use: turret detail; crew dress.

**[a3-05-04]** `a3_05_vehicles_wrecks/a3-05_tiger2-with-crew-in-street_barch-101i-680-8282a-16a.jpg`
Bundesarchiv Bild 101I-680-8282A-16A, Faupel, October 1944, Budapest. Grade A. **[eq]**
Shows: a Tiger II in a street beside two-storey baroque buildings, crewmen on the turret and one sitting
astride the barrel.
Use: tank against buildings and people.

**[a3-05-05]** `a3_05_vehicles_wrecks/a3-05_tiger2-column-on-wet-road_barch-146-1975-102-12a.jpg`
Bundesarchiv Bild 146-1975-102-12A, Hamann, 1944. Grade A-. **[eq]** [prop]
Shows: Tiger IIs in column on a wet road across flat country, commanders in the hatches, mud on the hulls.
Use: column spacing (about 20-30 m); wet mud finish.

**[a3-05-06]** `a3_05_vehicles_wrecks/a3-05_tiger2-passing-prisoner-column_nara-111-sc-198241.jpg`
German military photograph captured by the US Army (NARA 111-SC-198241), 16 December 1944, near
Merlscheid (s.SS-Pz.Abt. 501). Grade A. **[eq]** [winter]
Shows: a Tiger II on a muddy road passing a long line of prisoners; telegraph poles; open fields with snow
patches.
Use: an SS heavy-tank battalion's Tiger II on a country road; road width against the tank.

**[a3-05-07]** `a3_05_vehicles_wrecks/a3-05_abandoned-guns-and-trucks-on-retreat-road_1945-02-19.jpg`
Boris Velyashev, 19 February 1945, Elbing (Estonian History Museum print). Grade A-. **[eq]** [prop]
Shows: Pak 40 anti-tank guns and an 88 mm Flak 36 with patchy whitewash left on a muddy street by bare
trees and brick sheds; sepia print.
Use: abandoned guns as a trace of a collapsing column.

### 3.6 People, refugees and prisoners (`a3_06`, 6)

**[a3-06-01]** `a3_06_people_refugees/a3-06_refugees-and-soldiers-on-forest-road_barch-146-1976-072-09.jpg`
Bundesarchiv Bild 146-1976-072-09, February 1945, near Braunsberg (East Prussia). Grade A-. **[eq]**
[winter]
Shows: a packed forest road between pines: women, children and soldiers walking together, horses and a
wagon, a motorcycle combination, deep ruts in sand.
Reproduce: soldiers and civilians on the same forest road, in the same direction. The Halbe pocket held
many civilians with the army (published figures vary; not verified in this set); this is the closest
open photograph of that condition.
Use: the breakout's human composition (for traces: carts, bundles, prams among military wrecks).

**[a3-06-02]** `a3_06_people_refugees/a3-06_refugees-with-handcarts-through-village_iwm-b14736.jpg`
IWM B 14736, Sgt Midgley, 19 February 1945, Bedburg near Kleve. Grade A. **[eq]**
Shows: civilians with a handcart, bags and a milk can walking past soldiers in a muddy village street,
a brick villa with a verandah and pines behind.
Use: handcart form; villa-and-pine village edge like Halbe's.

**[a3-06-03]** `a3_06_people_refugees/a3-06_civilians-with-bundles-leaving-town_1945-04-09.jpg`
T-4 Irving Leibowitz (US Army), 9 April 1945, Heilbronn. Grade A- (small file). **[eq]**
Shows: women and men carrying bags, bedding and a folded blanket along a street of damaged houses.
Use: April 1945 civilian dress and loads.

**[a3-06-04]** `a3_06_people_refugees/a3-06_civilians-returning-through-ruins_1945-02.jpg`
Unknown photographer, February 1945, Elbing. Grade B. **[eq]**
Shows: women and a boy with suitcases and a pram-cart on a wet, rubble-strewn street between gutted
buildings with bare roof frames; poles and wires.
Use: after the fighting: people return to ruins (after the act's timeline).

**[a3-06-05]** `a3_06_people_refugees/a3-06_prisoners-through-fortress-gate_1945.jpg`
Semyon Alperin, between 23 February and 5 March 1945, Graudenz (Russian State Museum Catalogue). Grade A-.
**[eq]** [prop]
Shows: prisoners in greatcoats with bundles and a mess tin filing out of a brick fortress gate with a
readable inscription.
Use: the state of men leaving a pocket; not a Halbe place.

**[a3-06-06]** `a3_06_people_refugees/a3-06_prisoner-column-on-open-road_1945.jpg`
Emmanuil Evzerikhin, March-May 1945, East Prussia. Grade B. **[eq]**
Shows: a column of prisoners four or five abreast on a road across open flat fields, bare birches along
it, a wood line on the horizon, bright sun.
Use: the end of the pocket for most of those who survived it.

### 3.7 Spring weather and light (`a3_07`, 2)

**[a3-07-01]** `a3_07_spring_weather_light/a3-07_smoke-over-burning-town-street_1945-04-16.jpg`
US Army, 16 April 1945, Waldenburg (Württemberg). Grade A-. **[eq]** [prop] (the caption uses a
derogatory term).
Shows: an elevated view down a street of burnt-out houses in rows, chimneys and wall ends standing,
smoke rising in columns and hanging over the roofs, a church tower dissolving into the smoke, two small
figures in the street.
Reproduce: a burning small town from above: roofless rows, chimneys, smoke columns that merge into a
layer; the tower 150 m away half hidden.
Use: burning-town atmosphere and depth.

**[a3-07-02]** `a3_07_spring_weather_light/a3-07_burning-vehicle-smoke-in-suburban-trees_1945-04.jpg`
Georgy Ugrinovich, April 1945, a Berlin suburb, 7th Guards Tank Corps. Grade B. **[eq]**
Shows: a burning car wreck on a cobbled road at the foot of a wooded slope, flames and black smoke rising
through trees in fresh leaf; a T-34-85 and a truck beyond; a soldier waving; a stone gate pillar.
Reproduce: late April: deciduous trees in fresh small leaf, pines dark; a burning vehicle's smoke is
black and rises steeply in still air.
Use: late-April leaf state; vehicle fire.

## 4. Visual rules, Act 2: the Oderbruch

Each rule is numbered and cites its photographs. "Must" is what a generator or render has to satisfy;
"should" is a strong default that the component owner may overrule with a better cited source, saying so.
Numbers are estimates with the uncertainty given in the catalogue entry. A rule that rests only on
equivalents is marked **(eq only)**: replace it with an in-region source when one is found. No rule sets
an albedo (§1.4). Where the DEM, the Sentinel-2 imagery or the 1934 Messtischblatt 3452 disagree with a
rule on layout, the map and the DEM win; the photographs govern form, damage and light.

### 4.1 Buildings (B2)

**B2-1. Village grammar.** Oderbruch villages must be built of 19th-century red brick and rendered
masonry buildings of one and two storeys with round- or segmental-arched openings, half-timbered barns
and houses with brick infill, brick yard walls closing each Gehöft to the street, and large street trees
([a2-01-03] eq, [a2-01-05] eq, [a2-01-01]). Roofs pitch 40-50 degrees ([a2-01-05]). The Gehöft layout
itself (house, barn and stables round a yard) is not photographed: take it from the Messtischblatt and
building-history sources.

**B2-2. The leveled home is low.** A farmhouse destroyed in the April battle must read as a rubble field
0.5-1 m deep over its own footprint, with wall stubs up to 1 m, a charred post or chimney stack
standing, and no roof timber left ([a2-05-01], [a2-07-04] eq, [a2-05-04]). Standing full-height shells
are the exception in the Oderbruch photographs, not the rule; from the fields a leveled village is a pale
broken band 3-8 m high under a taller skyline of dead trees ([a2-01-01]).

**B2-3. The village church survives as a shaft or a gable.** Church towers must be able to stand hollow
after the roof, bell frame and floors burn: a ragged top with iron beam ends, empty openings, the porch
and door frame surviving at the foot ([a2-05-03]); a shaped nave gable can stand alone 100 m across a
rubble field ([a2-05-01], [a2-05-02]). This is `Village Church` in `world.rs`.

**B2-4. Roofs fail first.** Tile roofs lose their tiles and show battens before walls fail; burnt roofs
leave chimney stacks standing along the ridge ([a2-01-05] eq, [a2-05-04]). While a brick house burns,
black smoke pours from its windows (the contents burn, not the walls) and domestic iron such as
bedsteads survives in the rubble ([a2-05-05] eq).

**B2-5. Railway buildings.** The Küstrin-Frankfurt line's stations at the foot of the scarp are single-
storey brick buildings about 20 m long with a brick platform edge; destroyed, they keep walls and chimneys
but lose roof and windows ([a2-05-04]).

**B2-6. What survives among ruins: timber huts.** Wartime timber barracks (about 30 m long, tar-paper
roofs, stove chimneys every 4-5 m) can stand intact among burnt brick ([a2-01-02]).

**B2-7. Farm fittings.** Farmyards carry free-standing domed bread ovens (about 1.2 m high, 2 m long,
[a2-01-07] eq), barn gateways with hoist hooks and sack barrows ([a2-01-04] eq), thatched sheds at field
edges ([a2-01-06] eq). **(eq only)**

### 4.2 Terrain and vegetation (T2)

**T2-1. Dead flat to the horizon.** The polder floor must read dead flat: the only verticals are village
tree lines at 1-3 km, poles and single trees; a wood line at 1-2 km is a thin dark band
([a2-02-02], [a2-02-05], [a2-06-06] eq).

**T2-2. April fields are bare earth.** In mid-April the arable fields are ploughed or sown bare earth
that takes every tracked vehicle's imprint (ruts 0.1-0.3 m, track pattern visible) ([a2-02-02]); grass
and meadow are short and pale ([a2-08-03] eq, [a2-03-07]). Colour and green-up must come from the
2 April 2025 Sentinel-2 scene, not from these prints.

**T2-3. The Oder's meadows are flooded.** In February-April 1945 the Oder's floodplain carried open water
up to the eaves of low buildings and around fence lines; dikes are the dry lines ([a2-03-02], [a2-03-05],
[a2-02-01]). The river is 150-250 m wide at Küstrin and Frankfurt ([a2-02-01], [a2-03-05]); ice floes
drift in February ([a2-03-03], [a2-03-04]).

**T2-4. Avenues (Alleen).** Roads run between rows of large old trees (trunks 0.6-1 m, spacing 8-12 m)
([a2-06-01], [a2-06-04] eq, [a2-06-05] eq). Where the barrage passed, the avenue trees and village trees
must be shell-stripped: trunk and main limbs standing, crowns and twigs gone, dead for months
([a2-06-01], [a2-01-01], [a2-05-02]).

**T2-5. Early-April leaf state.** In the first week of April deciduous trees and birches are bare
([a2-01-07] eq, [a2-08-02] eq); by 20 April shrubs and young trees are in fresh leaf ([a3-03-01] eq).
The Seelow battle (16-19 April) falls between: buds and first leaf on shrubs, trees mostly bare.

**T2-6. The scarp is not photographed.** Its form must come from the GLO-30 DEM and the Messtischblatt;
the observation posts on the Reitwein spur and the heights ([a2-04-01], [a2-04-02]) only show that posts
were dug in and roofed with net and cut pine.

**T2-7. Field and ditch pattern.** Fields are long parcels 50-200 m wide; farmsteads string along roads;
watercourses keep a meadow band ([a2-02-03] eq, [a2-02-04] eq). **(eq only; the Messtischblatt governs.)**

### 4.3 Dressing and wrecks (D2)

**D2-1. River crossings.** Soviet combat bridges are low timber trestles on driven piles, deck under 1 m
above water, bents 4-6 m apart, no rails ([a2-03-03]); pontoon bridges are built beside blown steel
bridges whose spans lie angled in the river ([a2-03-01]). Canal bridges fail by dropping the deck into
the channel between intact abutments ([a2-03-09] eq). The `Canal Bridge` landmark should use this failure.
Permanent timber pile bridges carry raked ice-breakers upstream of each pier ([a2-03-06]); river towns
meet the water with timber quays and landing stages ([a2-03-08] eq).

**D2-2. Bank positions.** German positions on the river are cut into the bank or dike and revetted with
planks, with an MG table and spoil ([a2-03-04], [a2-03-05]); Soviet observation posts are dug in and
roofed with fishing-net camouflage over cut pine boughs ([a2-04-01], [a2-04-02]).

**D2-3. Gun lines.** Soviet artillery stood wheel to wheel along field edges at 20-30 m intervals with
ammunition boxes stacked by each gun and slit trenches beside them ([a2-02-05]); after they moved on, the
traces are empty boxes, spoil and ruts. Heavy howitzers leave deep track ruts across open fields
([a2-02-02]).

**D2-4. Armour on the roads.** T-34-85s carry infantry riders on every surface ([a2-06-01], [a2-06-05]);
heavy tank columns move nose to tail with 2-5 m gaps ([a2-06-04]); the Act 2 boss (ISU-152) is a low
casemate with a thick gun 2.5-3 m proud of the hull, people shoulder-high to its track guards
([a2-06-03]). Tank turrets carry painted slogans and numbers that must not be legible.

**D2-5. Roadside debris.** Verges carry what defenders dropped: Panzerfaust tubes, boxes, sheet metal
([a2-06-04]); heavy traffic and shelling tear cobbles into loose heaps at the kerb ([a2-06-02]).

**D2-6. Knocked-out heavy tanks in fields** sit where they stopped with a track run off, no crater round
them, the only object for hundreds of metres ([a2-06-06] eq). Moving a turret needs a crane
([a2-06-07] eq).

### 4.4 Weather and light (W2)

**W2-1. River mist and haze.** Over the flooded Oder, mist halves contrast at 50-80 m and the far bank is
a grey silhouette ([a2-03-01], [a2-03-02]). The participating medium must produce this; no post-process
veil.

**W2-2. The night barrage is lit by its weapons.** Rocket streaks and muzzle flashes light the smoke from
inside, and the smoke scatters that light onto the ground and houses ([a2-08-01]). Searchlights were
used on 16 April; no photograph of them was found, so their beam form must come from physical modelling,
not from an image.

**W2-3. Smoke from burning villages** is dense and low; contrast falls to near zero within about 50 m;
chimneys and poles are the last verticals to vanish ([a2-08-02] eq, [a2-06-02] eq). Backlit smoke glows
bright; smoke in shadow is grey ([a2-01-05]).

**W2-4. April daylight** on open ground: a pale, hazy sky, low sun at morning and evening with long
shadows across bare fields ([a2-02-02], [a2-08-03] eq, [a2-01-05] eq); overcast days flatten everything
to the soft grey of the Donath views ([a2-01-01], [a2-05-01]).

### 4.5 Physical traces of human events (H2)

**H2-1. The home as rubble and dead trees** ([a2-05-01], [a2-01-01]): the player must be able to read
"this was a village" from a church gable, chimney stacks, wall stubs in a grid of plots, and a skyline of
shell-stripped trees; no text, no signs.

**H2-2. Treks leave objects.** Along roads and at village edges: loaded farm wagons, bedding tied in
sheets, suitcases, baskets, milk cans, handcarts and child-size carts ([a2-07-01], [a2-07-03],
[a2-07-04], [a2-07-05]). A single small cart by a bench carries the whole event ([a2-07-05]). A halted
trek packs hundreds of covered wagons 1-3 m apart ([a2-07-02] eq); people ride wrapped in bedding on the
loads ([a2-07-06] eq); retreating military vehicles carry soldiers, bundles and civilians on every
surface ([a2-07-10] eq).

**H2-3. The last levy.** Volkssturm and Hitler Youth defended the Oder line and the villages
([a2-03-04], [a2-03-05], [a2-07-09] eq): traces are revetted bank positions, slit trenches at field edges
([a2-07-07] eq), Panzerfaust tubes (each defender carried one or two, [a2-07-08] eq), oversized helmets
and coats. These photographs carry propaganda captions; the traces must be material, restrained,
and never staged as heroism.

**H2-4. The attacker's traces** across the plain: gun positions with empty boxes and spoil, track ruts,
timber crossings, dug-in observation posts ([a2-02-05], [a2-02-02], [a2-03-03], [a2-04-01]).

**H2-5. The dead are not in this set.** No photograph shows the battle's dead; do not invent scenes of
bodies from these references. Where the act needs the dead, source them separately and cite the event.

## 5. Visual rules, Act 3: Halbe

### 5.1 Buildings (B3)

**B3-1. Small-town frontage.** Streets of continuous two- and three-storey rendered or brick houses with
pedimented or plain window heads and dormers, windows about 1.0 x 1.8 m on 2.5-3 m axes, cobbled
carriageways 8-10 m wide ([a3-02-04] eq, [a3-02-02] eq, [a3-02-08] eq). **(eq only.)** Halbe itself was
smaller and looser than these Pomeranian towns: take its plot pattern from the 1941 Messtischblatt 3848.

**B3-2. The forest edge is a villa colony.** Detached two-storey houses with steep roofs stand back in
gardens with pines left in the plots; cast-iron lamp standards, low garden walls ([a3-02-01] eq,
[a3-06-02] eq).

**B3-3. Brick public buildings** (church, station) are North German brick with pointed or round arches
([a3-02-05] eq, [a2-05-03], [a2-05-04]). The railway through the town brings semaphore gantries,
ballast and, if a train was caught, a locomotive as a large obstacle ([a3-02-09] eq).

**B3-4. How the town fails.** Plaster ceilings and lath fall first ([a3-02-03]); facades open to show
floors, shutters and balconies hang ([a3-04-01]); rubble spills from the plots onto the verges while a
lane stays clear for traffic ([a3-02-07]); burnt blocks keep front walls and stepped gables as
silhouettes ([a3-04-04], [a3-02-10]); a heavy tank that backs into a masonry house breaks the lower front
and buries its own engine deck ([a3-04-02]) - an outcome the destruction simulation must be able to
produce, not a scripted pose.

**B3-5. Barricades** are household material: doors, boards and corrugated sheet propped 2-2.5 m high
against house walls at junctions ([a3-02-06] eq).

### 5.2 Terrain and vegetation (T3)

**T3-1. The pine stand.** Scots pine on sand: trunks straight (managed stands, [a3-01-04], [a3-01-02]) or
crooked (heath pine, [a3-01-01]), bare for most of their height with crowns in the top third, 15-20 m
tall, spacing 2-4 m in young stands and 4-8 m in old; no shrub layer; a floor of needles, dead grass and
some leaf litter. Sight lines inside the stand 30-80 m. **(eq only: Reichswald and Celle.)**

**T3-2. Sand everywhere.** Every cut (trench, crater, rut, road bank) must expose pale sand against dark
litter ([a3-01-09], [a3-01-05], [a3-03-09]). Sand roads under tracked traffic become paired ruts
0.2-0.4 m deep with a raised crown ([a3-01-03], [a3-06-01]).

**T3-3. Forest roads and rides** are straight corridors 6-10 m wide between walls of trunks, ending in
haze ([a3-01-02], [a3-05-01]); forest blocks are divided by a rectangular grid of rides
([a3-01-10] eq map). Narrow tracks one vehicle wide trap columns ([a3-01-07]). At clearing edges, slash
and young pine cut sight lines to a few metres ([a3-01-06]).

**T3-4. Late-April leaf.** Birches and shrubs in first, small leaf; pines dark; oaks later
([a3-02-01], [a3-07-02], [a3-03-01]). Late-April weather at Halbe was dry and forest fires burned in the
pocket (standard accounts; no photograph in the set shows a forest fire).

**T3-5. The Autobahn.** A dual concrete carriageway with visible slab joints, grass verges 2-3 m with a
drainage edge, a central reservation, concrete overbridges every few kilometres with 4.5-5 m clearance,
embankments 4-5 m high with a fence on the crest, and, where it cuts forest, a tree wall 10-20 m from the
edge ([a3-03-02], [a3-03-04], [a3-03-06], [a3-03-08]); junction boards and kilometre stones stand on the
verge ([a3-03-05]). **(eq only: Bremen-Hamburg, Lübeck, Frankfurt,
Munich sections; the Reichsautobahn standard applies to the Berlin-Dresden line at Halbe.)**

### 5.3 Dressing and wrecks (D3)

**D3-1. The breakout's forest road.** Wrecked and burnt vehicles pushed together along a sandy forest
road under undamaged pines: civilian cars, vans and military trucks mixed, contents spilled around
them ([a3-05-01]). This is the act's primary scene reference; its provenance is weak (Grade C), so its
details should be corroborated before a scene is built on them alone.

**D3-2. Abandoned where they stopped.** Equipment pulled off the track under trees and left
([a3-01-01]); guns left hitched or unhitched on streets ([a3-05-07]); columns jammed nose to tail on a
narrow track ([a3-01-07]).

**D3-3. The Tiger II.** Production turret, Zimmerit to autumn 1944, spare track links on the turret
sides, soft three-tone camouflage, mud on the lower hull; column spacing 20-30 m ([a3-05-02], [a3-05-03],
[a3-05-05]). No photograph of s.SS-Pz.Abt. 502's Tigers exists in the set: markings and finish of the
Halbe tanks are unknown here.

**D3-4. Scale in streets.** A Tiger II or IS-2 nearly fills an 8-10 m street ([a3-04-01], [a3-02-02]);
people are waist-high to its track guards ([a3-05-04]); on a country road it leaves no room beside it for
a column of men ([a3-05-06]).

**D3-5. The blocking line on the Autobahn.** Tanks and guns sit at overbridges and behind the embankment;
infantry dig foxholes into the embankment face ([a3-03-01], [a3-03-06]). Small bridges on the approaches
fail under tanks ([a3-03-03]); demolished viaducts drop whole spans ([a3-03-07], a form the flat Halbe
section does not need). A burnt tank left at a town edge serves as a road block ([a3-04-03]).

**D3-6. Soviet armour in the forest and suburbs** of the south of Berlin: T-34-85s with riders on narrow
pine-forest roads ([a3-01-08]), T-34-85s and IS-2s in villa streets ([a3-02-01], [a3-07-02]).

### 5.4 Weather and light (W3)

**W3-1. Smoke over a burning town** rises in columns and merges into a layer; a tower 150 m away is half
hidden ([a3-07-01] eq). Vehicle fires give black smoke rising steeply in still air ([a3-07-02]). Both
must come from the participating medium and emitters (the fires), not overlays.

**W3-2. Forest haze.** Down a straight forest road, vehicles at 100-200 m are grey ([a3-01-02]); mist
among trunks on damp mornings ([a3-01-07], [a3-02-01]).

**W3-3. Light inside the stand.** Sun comes through the thin crowns in hard patches on a bright floor
([a3-01-09], [a3-01-01]); on overcast days the stand is evenly grey with dark trunks against a pale sky
([a3-01-04]).

### 5.5 Physical traces of human events (H3)

**H3-1. Civilians and soldiers together.** The pocket's forest roads carried soldiers, women, children,
horses and carts in the same stream ([a3-06-01] eq). Traces: prams, handcarts, bundles and suitcases
among military wrecks ([a3-06-02], [a3-06-04], [a2-07-01]); April civilians carried bags, bedding and
folded blankets ([a3-06-03]).

**H3-2. The end of the column.** Burnt vehicles with spilled contents, pushed off the road
([a3-05-01]); guns abandoned on the road ([a3-05-07]); prisoners in long columns on open roads afterwards
([a3-06-06], [a3-06-05]).

**H3-3. Barricades and fighting positions** in the town (doors and boards, [a3-02-06]) and in the forest
(sand trenches with fresh pale spoil, [a3-01-05], [a3-01-09]).

**H3-4. Restraint.** As in Act 2, the set shows no dead; Halbe's dead are commemorated in Germany's largest
war cemetery there, which is postwar and outside this set. Do not stage bodies from these references.

## 6. Photographs to compare renders against

Each row names the 2-3 photographs a render of that environment should be put beside, and what to check
first. Headline references: **Act 2** - [a2-05-01] (the leveled Podelzig), [a2-02-05] (the open plain
with gun lines), [a2-03-02] (the flooded Oder bank). **Act 3** - [a3-05-01] (the wrecked convoy in the
pines), [a3-01-01] (the April pine heath), [a3-03-06] (the Autobahn embankment as a blocking line).

| Environment | Compare against | Check first |
|---|---|---|
| Act 2: leveled Oderbruch village (the home) | [a2-05-01], [a2-01-01], [a2-05-03] | low rubble field, church shaft or gable, dead-tree skyline |
| Act 2: village street before destruction | [a2-01-03], [a2-01-05], [a2-07-04] | red brick with arched openings, half-timber, yard walls, big trees |
| Act 2: open field grid in April | [a2-02-02], [a2-02-05], [a2-02-01] | dead-flat bare earth, ruts, tree lines at 1-3 km |
| Act 2: the Oder bank and flooded meadows | [a2-03-02], [a2-03-05], [a2-03-04] | dike crest, water to the eaves, 150-250 m river |
| Act 2: crossings and canal bridge | [a2-03-03], [a2-03-01], [a2-03-09] | low pile trestle, blown truss in the river, dropped deck |
| Act 2: the scarp's observation posts | [a2-04-01], [a2-04-02] | net over cut pine, plank parapet (landform from the DEM) |
| Act 2: avenues and armour on the roads | [a2-06-01], [a2-06-04], [a2-06-03] | shell-stripped avenue, riders, nose-to-tail column, ISU bulk |
| Act 2: railway at the foot of the scarp | [a2-05-04] | single-storey brick station, stacks, bare track bed |
| Act 2: smoke, fire and the night barrage | [a2-08-02], [a2-08-01], [a2-03-01] | contrast loss at 50 m, smoke lit by its sources, river mist |
| Act 3: pine stand | [a3-01-01], [a3-01-04], [a3-01-02] | trunk height and spacing, crowns in the top third, open floor |
| Act 3: forest road after the breakout | [a3-05-01], [a3-01-03], [a3-06-01] | mixed wrecks under intact pines, sand ruts, depth haze |
| Act 3: town street | [a3-02-04], [a3-02-02], [a3-02-01] | frontage grammar, tank-to-street scale, villa edge with pines |
| Act 3: town under fighting | [a3-04-01], [a3-02-08], [a3-02-06] | opened facades, rubble layer, household barricades |
| Act 3: burnt town silhouette and smoke | [a3-04-04], [a3-07-01] | gables against sky, smoke layer hiding the tower |
| Act 3: the Autobahn blocking line | [a3-03-06], [a3-03-01], [a3-03-02] | embankment foxholes, overbridge, slab joints, verge |
| Act 3: Autobahn through forest | [a3-03-08], [a3-03-04] | tree wall 10-20 m from the edge, overbridge clearance |
| Act 3: the platoon's Tiger IIs | [a3-05-02], [a3-05-03], [a3-05-05] | silhouette, Zimmerit, track links, column spacing |
| Act 3: sand in cuts and craters | [a3-01-09], [a3-01-05] | pale sand under dark litter |
| Act 3: late-April light and leaf | [a3-07-02], [a3-02-01] | first leaf on birches, dark pines, black vehicle smoke |

## 7. What the set cannot tell

- **Halbe itself.** One weak-provenance photograph of the pocket ([a3-05-01]) and one link-only record
  (Forsthaus Halbe). No wartime photograph of Halbe's streets, church, station, the Lindenstraße
  barricade, the A13 blocking line near Teupitz and Halbe, or Märkisch Buchholz.
- **s.SS-Pz.Abt. 502 in 1945.** No photograph of its Tiger IIs, their markings or their wrecks.
- **The Seelow Heights as a landform** and the town of Seelow, Friedersdorf, Dolgelin, Werbig, Golzow or
  Sachsendorf in 1945: none found. The scarp must come from the DEM and the Messtischblatt.
- **The Oderbruch's drainage canals** (Alte Oder, Hauptgraben) and their bridges: none found; the canal
  rule rests on a Polish canal ([a2-03-09]).
- **`world.rs` landmarks not covered:** the windmill, the water tower and a manor house of the Oderbruch
  have no 1944-45 photograph in the set (the three-storey block in [a2-01-05] is not identified as a
  manor).
- **Ground-level April farmland in the Oderbruch**: only the plain in general ([a2-02-02], [a2-02-05],
  both place unverified) and the Küstrin aerial.
- **The searchlights of 16 April, forest fires in the Halbe pocket, and the dead of both battles** are
  not photographed here.
- **Colour.** All prints are black and white or sepia; green-up, soil and brick colour must come from the
  Sentinel-2 scenes (2 and 4 April 2025) and physical data.
- **Dates of several Soviet photographs** rest on secondary websites (waralbum.ru, tankfront.ru, book
  scans); where a rule depends on them it cites an archive-grade photograph as well.

