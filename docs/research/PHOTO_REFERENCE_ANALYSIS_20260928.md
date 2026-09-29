# Wartime photographs as 3D reference: urban intro district and the maps beyond

Role P, urban-intro city team, 28 September 2026. Reference set:
`/home/alex/panzer/reference/wartime_photos_20260928/` (README.md, manifest.json, index.html).
Contact sheets: `/home/alex/.claude/jobs/ca78baf9/tmp/research/photo_boards/`.

## Summary

All 180 photographs in the set's current `manifest.json` were looked at one by one (the Bundesarchiv
files at their full 800 px, the larger renditions scaled to the viewer, and 17 of them again in
full-resolution crops where a measurement or a detail needed it). Each is catalogued in section 2
with its source, date, place, reliability grade, what it shows, what the 3D world must reproduce,
and scale estimates with their uncertainty. Section 3 turns them into 72 numbered rules for the
grammar (G1-G20), the district (L1-L14), the story (S1-S14), the verifier (V1-V13) and the maps
beyond the district (M1-M11); every rule cites its photographs and every photograph is cited by at
least one rule. Section 4 names the photographs each typology, condition and environment must be
compared against, and section 5 lists what the set cannot tell.

What matters most:

1. **The centre in January 1943 is an open snowfield with isolated standing masses**: gutted shells,
   single-bay fragments of three to five storeys, and chimney stacks, with 50-250 m of trodden snow
   between them, not continuous street walls (L1: [04-01], [06-12], [04-02]).
2. **Masonry stands without its floors.** Burned brick buildings are full-height shells with sky in
   every window; the structural model must let walls stand on walls and cross walls after every
   floor has gone (G8, G10, V5).
3. **The floor construction decides the failure.** Timber floors and roofs vanish; concrete slabs,
   stairs and balconies stay, project or hang; steel joists stick out of walls. The blueprint needs
   each floor's construction (G9: [04-09], [07-16], [04-04], [04-02]).
4. **Timber quarters burn down to their stoves and chimneys**, in rows at plot pitch, with streets
   surviving as tracks across the snow (G13, L2: [04-27], [04-07], [04-23]).
5. **Measured proportions differ by building age.** A 1930s block has a storey pitch of about
   3.0-3.6 m, a ground floor raised about 1 m over a cellar (first balcony at 4.0-4.6 m, measured on
   the brick courses), and windows of about 1.3-1.45 x 1.45-1.6 m on 2.7-3.4 m axes; older brick has
   taller, arched windows. The contract's 4 m bay with one window is sparser than any photographed
   1930s block (G1, G2: [04-11], [04-09], [04-05], [07-07]).
6. **Damage is geometry.** Breaches step back along the brick bond; fragments pit whole walls;
   render and the outer half-brick spall round openings; rubble comes from failed volume and lies
   mostly inside the footprint with a 1-2 m berm outside (G11, G14, G17).
7. **Snow**: 5-15 cm in the centre, trodden to a crust with debris on top; 20-30 cm on squares;
   drifted smooth over rubble inside roofless buildings; on every ledge and cornice (L8, V11).
8. **Light**: overcast frames lose the horizon; in low-sun frames the snow's shadows stay well above
   black beside the brightest lit snow; contrast falls visibly at 100-500 m, which the participating
   medium must produce (V2-V4).
9. **The set is silent on** loopholes and blocked windows in Stalingrad, street barricades, craters
   in the city, wall sections, winter colour, and any Tiger near Stalingrad (section 5). These must
   come from R1's cited sources or physical data.

Contact sheets (all in `photo_boards/`, rebuilt deterministically by
`photo_boards/tools/make_boards.py`; internal reference only, several photographs are CC BY-SA):

- `board_01`-`board_14`: theme boards (merchant houses, apartment blocks, timber houses and
  villages, workshops and industry, damage, streets and squares, snow and light, cellars and
  positions, story traces, the steppe and pocket, the player Tiger, Soviet armour and guns, people,
  later acts), each tile labelled with its id, caption, date, place, grade and credit.
- `compare_typologies.jpg`, `compare_environments_district.jpg`, `compare_environments_beyond.jpg`:
  one row per subject of section 4, with an empty slot at the end of each row; `make_boards.py
  --render "<row label>=<png>"` puts a render into it for G, L and V.
- `index_all.jpg`: all 180 photographs by id. `detail_measurements.jpg`: the crops that carry a
  measurement, with pixel rulers.

## 1. Method

### 1.1 What was looked at

Every downloadable record in `manifest.json` was opened with an image viewer, one photograph at a
time, and read against its manifest caption, date, place, photographer and archive id. The
photographs are identified here as `[FF-NN]`: FF is the folder number, NN the photograph's position
inside that folder in `manifest.json` order. Section 2 lists each one with its path.

The set as analysed (28 September 2026, 22:00-23:00):

- `manifest.json` holds 180 downloadable records and 8 link-only records. All 180 photographs were
  looked at. The link-only records (Vorovsky Street, the Banny ravine railway bridge, the Krasny
  Oktyabr panorama and Pavlov's House in folder 04; two Seelow Heights exhibition images, Forsthaus
  Halbe and a Bundesarchiv Reichstag image in folder 10) were not downloaded, so they are not
  analysed here; their captions and host pages are in the manifest.
- While this analysis ran, the set's own builder (`_work/build.py`, later documented as `_build/`)
  re-downloaded every file as a larger rendition (Wikimedia's 1920 px standard size where the
  original is wider) and rewrote `manifest.json` at 22:17 and 22:29. Every entry below was checked
  against the file on disk after that re-download, and all 180 `sha256` values match the 22:29
  manifest. One file, [08-04], was renamed by the builder; its entry gives the current name.
  Per-folder numbering is unchanged by the rewrites.
- A previous run of this role wrote the entries for [03-01]-[03-06], [04-25]-[04-28], folders 05 and
  06, and [08-01]-[08-14] (except [08-04]) from earlier renditions. Each was re-checked against the
  current file and corrected where the photograph disagreed; the corrections mostly concern scale
  readings taken from a reference at a different depth ([03-02], [03-04], [04-27], [04-28], [05-06],
  [05-07], [06-01], [06-06], [06-13], [08-12], [08-13]) and a few misread details ([03-03], [05-01],
  [05-10], [05-12], [05-14], [06-08], [06-10], [06-11], [08-05], [08-10], [08-11], [08-14]).
- Where a number needed it, full-resolution crops were made with a pixel ruler, and brick-course
  pitch was measured by autocorrelation of the wall's vertical profile. The crops that carry a
  measurement are kept on `photo_boards/detail_measurements.jpg`. The tools are in
  `photo_boards/tools/`: `make_boards.py` builds every board; `meta.py 04-11` prints a
  photograph's manifest record by id; `ruler.py` and `course.py` make the ruler crops and the
  course-pitch measurements; `crosscheck.py` checks this catalogue against the current manifest
  (paths, archive ids, photographers, years, id order) and `refcheck.py` checks every citation,
  that every rule cites photographs, and that section 4 matches the comparison boards. All read
  the set without writing to it.

### 1.2 Reliability grades

| Grade | Meaning |
|---|---|
| A | Archive record (Bundesarchiv, RIA Novosti, NAC, IWM, NARA) giving photographer, a dated caption (day or month) and a place. Bundesarchiv PK dates such as 1942-09-23, 1943-06-21 and 1943-09-23 are batch dates; the caption or print strip wins. |
| A- | Archive record whose date is a year, a season or a batch date only. |
| B | Named photographer with month and place, from the TASS/Commons description or a publication date; or an archive record whose place or exact date is missing. |
| C | Year only, anonymous, a book or web scan, or no place. Use for form, not for dating a scene. |

Flags: **[prop]** propaganda caption; **[staged?]** the framing or action looks arranged;
**[print]** contrast or retouch in the print makes tone unreliable; **[colour]** an original Agfa
colour slide whose dyes have shifted (usually toward cyan), so it gives relative value and hue
families, never albedo.

### 1.3 Measuring from photographs

Scales came from known objects in the frame: a standing man 1.70-1.80 m; a door 2.0-2.2 m; a T-34
3.0 m wide and 2.4 m tall (hull roof about 1.6 m); a Tiger I 3.7 m wide on battle tracks (the
brief's figure; published figures run 3.56-3.7 m, and about 3.1 m on the narrow transport
tracks), 3.0 m tall, hull 6.3 m long; a Russian brick course about 77 mm (250 x 120 x 65 mm brick
with a 10-12 mm joint); a rail wagon wheel 0.9-1.05 m. Heights are read where the reference stands
at the same depth as the measured object; otherwise the value is a perspective estimate. Every
figure carries its uncertainty. Where a figure depends on a single reference at a different depth,
the uncertainty is at least +/-30 %.

### 1.4 Tone, not albedo

All but seven images are black-and-white prints and scans of prints (one, [09-13], is a printed
page). They inform form, damage, composition, the relative tone of surfaces and the distribution of
light. They never set albedo: film response (orthochromatic or panchromatic), print contrast,
retouching and later scanning all change the grey values. Albedo comes from physical material data.
The seven Bundesarchiv Bild 169 colour slides ([02-20], [02-21], [04-27], [05-14], [05-15], [07-17],
[09-12]) are dye-shifted and only show which surfaces are lighter or darker than others under the
same light, and all seven are summer or autumn 1942: none shows winter colour.

## 2. Catalogue: every photograph

Each entry gives the source and archive id; date and place with a grade; what it shows; what the 3D
world must reproduce (geometry, damage, streets, snow, weather, people); scale estimates; and the
role(s) the photograph serves: G grammar, L district, S story, V verifier, M maps beyond the
district.

### 2.1 Folder 01: Tiger I (30)

No photograph in this folder shows a Tiger near Stalingrad: none was found (README, gaps), and no
Tiger fought inside the pocket (brief §2). These photographs set the player vehicle's geometry,
finish, wear and damage states and the crew's dress and routines. They must not be used to date or
place a Tiger in the city.

**[01-01]** `01_tiger_i/01_tiger-early-production-tunisia-crew-on-turret_barch-101i-554-0872-35.jpg`
Bundesarchiv Bild 101I-554-0872-35, Helmuth Pirath, January 1943, Tunisia (s.Pz.Abt. 501 according
to Commons). Grade A.
Shows: an early-production Tiger from the front left on a dirt track in hill country; two crewmen on
the turret roof beside the raised hatch lids; interleaved rubber-tyred road wheels; wide battle
tracks with dry mud caked in the links; the hull side painted with a unit sign.
Reproduce: the early Tiger silhouette (drum cupola, smooth turret sides, front mudguards); dry earth
packs into the track links and on the lower hull; the tracks leave shallow ruts in dry ground.
Use: V (player Tiger silhouette, early production), people (crew on the turret).

**[01-02]** `01_tiger_i/01_tiger-frontal-crew-in-hatches-1944_barch-101i-299-1805-16.jpg`
Bundesarchiv Bild 101I-299-1805-16, Scheck, 1944, northern France (1st SS Panzer Division, turret
number 323 per the archive; the crop hides the number). Grade A (a 1944 vehicle).
Shows: a Tiger coming head-on down a dirt road across open fields with its commander and loader up
in the turret hatches and the driver and radio operator with their heads out of the hull hatches;
spare track links hung across the lower hull front.
Reproduce: the head-on silhouette at eye level: the tracks take about 0.7 m of each side of the
3.7 m width; four heads out at once when not in action. Check any detail against 1942-43 production
before copying it.
Use: V (head-on silhouette), people.

**[01-03]** `01_tiger_i/01_tiger-winter-wash-beside-damaged-t34_barch-101i-457-0065-36.jpg`
Bundesarchiv Bild 101I-457-0065-36, Richard Kamm, 1942 (winter 1942-43), central or northern sector
(PK Luftflotte 1). Grade A- (year only).
Shows: two German soldiers on a Tiger seen from the rear quarter; beside it on a rutted dirt road a
T-34 with the hexagonal turret, both round roof hatches open, its tracks gone so that the road
wheels stand bare on the road, and rough white wash in broken patches on its hull and turret; a
staff car's mudguard in the foreground; a telegraph pole with wires, bare shrubs, thin snow on the
verge.
Reproduce: a T-34 that has lost its tracks sits about 0.1-0.2 m lower on its bare wheels; winter
wash on both sides' tanks was brushed on in broken patches; roads are dark rutted earth with snow
only on the verges.
Use: V (whitewash in 1942-43; T-34 without tracks), S (a captured or knocked-out T-34 by a road).

**[01-04]** `01_tiger_i/01_tiger-323-patchy-whitewash-village_barch-101i-457-0056-12.jpg`
Bundesarchiv Bild 101I-457-0056-12, Richard Kamm, 1942 (winter 1942-43), central sector (s.Pz.Abt.
502 per Commons). Grade A-.
Shows: Tiger 323 in a village street; whitewash applied in irregular blotches over the dark base,
the turret number left dark; crewmen on the hull; log houses with thick thatched roofs; bare trees;
carts; a street of mud with thin snow.
Reproduce: the 1942-43 winter finish is a hand-applied, blotchy wash with large areas of the dark
base showing, not a uniform white. Thatch 0.3-0.5 m thick on steep roofs of about 45-50 degrees; log
walls with notched corners; eaves about 2.2-2.6 m (+/-20 %, against the man standing on the Tiger's
deck at a similar depth).
Use: V (primary whitewash reference for the player Tiger, 1942-43), M (northern villages; not the
Stalingrad steppe, where roofs are reed, straw or sheet).

**[01-05]** `01_tiger_i/01_tiger-crew-on-rear-deck-worn-white-patches_barch-101i-455-0020-18.jpg`
Bundesarchiv Bild 101I-455-0020-18, Zwirner, 1942. Grade A- (year only; the men in shirtsleeves and
bare trees point to spring).
Shows: three men at the Tiger's engine deck: one stripped to the waist, one in a shirt, a Luftwaffe
NCO; clothing, a mess tin and tools laid on the deck; bare trees on a slope behind.
Reproduce: the engine deck as the crew's table and wardrobe: clothes, tins and tools on the deck.
Use: S (crew life on the vehicle), people.

**[01-06]** `01_tiger_i/01_tiger-crew-cleaning-gun-barrel_barch-101i-455-0020-23.jpg`
Bundesarchiv Bild 101I-455-0020-23, Zwirner, 1942. Grade A-.
Shows: two men working a long cleaning rod into the muzzle of Tiger 314's gun, a third standing on
the hull; cut branches heaped against the hull and running gear for camouflage; a wood edge; the
road wheels pale with dust.
Reproduce: camouflage with cut branches laid against the hull; the cleaning rod is as long as the
barrel (about 5 m); the muzzle brake is at head height when the gun is level (about 2.2-2.4 m).
Use: S (crew routines), V (Tiger 314, camouflage branches).

**[01-07]** `01_tiger_i/01_tiger-behind-log-obstacle-spring-snow_barch-101i-276-0701-14.jpg`
Bundesarchiv Bild 101I-276-0701-14, Johannes Bergmann, spring 1943 (print strip), central sector.
Grade A.
Shows: a Tiger with its gun raised behind a position of logs and posts; its winter wash streaked and
worn to the base on the hull side and turret, spare track links on the turret side; three men; bare
birches; lingering snow in patches among dead leaves.
Reproduce: log obstacles and revetments of horizontal logs 0.2-0.3 m thick between vertical posts,
about 1-1.5 m high; worn winter wash streaks downward and wears away on edges, handholds and the
lower hull by spring.
Use: V (worn wash), L and M (log revetments and obstacles), S (a defended position).

**[01-08]** `01_tiger_i/01_tiger-231-side-profile-northern-front_barch-101i-698-0038-04.jpg`
Bundesarchiv Bild 101I-698-0038-04, Georg Schmidt-Scheeder, 1943 (date field 1943-06-21 is a batch
date), northern front. Grade A.
Shows: Tiger 231 in full side profile on flat grassland under cumulus: spare track links on the
turret side, a tow cable along the hull, the cross on the hull side, interleaved rubber-tyred road
wheels, the air filters at the rear; the commander standing in the cupola.
Reproduce: the side-profile proportions: hull length about 2.2 times the height to the turret roof
(6.3 m against 2.9 m), and the gun overhang about 2 m beyond the hull front with the turret at 12
o'clock (8.45 m overall against the 6.3 m hull; the photograph agrees).
Use: V (primary side-profile check for the player Tiger).

**[01-09]** `01_tiger_i/01_tiger-crew-applying-white-camouflage-paint_barch-101i-458-0076-37.jpg`
Bundesarchiv Bild 101I-458-0076-37, Stöpfgeshoff, winter 1943-44 (print strip). Grade A (a year
after Stalingrad).
Shows: a crewman brushing white paint by hand onto the turret side, streaked and thin, leaving dark
gaps round the smoke dischargers, fittings and hatch; another man holds a pole; snow and a box on
the deck; a pine behind.
Reproduce: whitewash is brushed: visible strokes, thin areas where the base shows through, dark
margins round fittings. It is a coating with thickness and wear, not a tint.
Use: V (whitewash texture), S (crew work).

**[01-10]** `01_tiger_i/01_tiger-winter-village-road-with-horse-carts_barch-101i-458-0076-31.jpg`
Bundesarchiv Bild 101I-458-0076-31, Stöpfgeshoff, winter 1943-44. Grade A.
Shows: a whitewashed Tiger with two crewmen on its turret on a snowy village road, beside it
soldiers leading a horse cart and a donkey team; pines; a log house; a truck covered with branches;
overhead wires.
Reproduce: a road of packed snow 5-10 cm thick with two dark ruts; the Tiger fills about half of a
7-8 m road; horse carts (wheels about 1 m, horses 1.5-1.6 m at the withers) share it; snow packs
white into the tracks while the lower hull goes grey with dirt.
Use: M (village road traffic), V (Tiger against horses for scale).

**[01-11]** `01_tiger_i/01_tiger-crew-eating-on-whitewashed-hull_barch-101i-277-0846-08.jpg`
Bundesarchiv Bild 101I-277-0846-08, Jacob, January-February 1944. Grade A.
Shows: four crewmen in reversible padded winter suits and caps eating from mess tins at the Tiger's
front; the wash worn off the gun barrel and the front-plate edges; a dark turret number; snow packed
in the tracks.
Reproduce: wear on the whitewash follows use: the barrel, hatch edges and the front plate where men
climb go dark first; the crew's padded suits, caps and felt boots.
Use: people (crew winter dress), V (wear pattern).

**[01-12]** `01_tiger_i/01_tiger-whitewashed-frontal-on-snowy-road_barch-101i-277-0846-12.jpg`
Bundesarchiv Bild 101I-277-0846-12, Jacob, January-February 1944. Grade A.
Shows: a whitewashed Tiger head-on on a snowy road with its commander and loader up; men on the rear
deck; a second Tiger on the left and a column behind; snow packed in every track link; overcast.
Reproduce: packed snow in the track links reads as a white chevron pattern; the dirty white hull
front is darker than fresh snow; a column spaces its vehicles about 10-20 m apart on a road.
Use: V (head-on winter Tiger), M.

**[01-13]** `01_tiger_i/01_tiger-s21-in-ruined-kharkov-square_barch-101i-704-0129-08.jpg`
Bundesarchiv Bild 101I-704-0129-08, Bauer-Altvater, 1943 (spring, after the recapture of Kharkov),
Kharkov (Rosa Luxemburg Square per Commons). Grade A. Kharkov, not Stalingrad.
Shows: Tiger S21 crossing a wide, pale, paved square with crewmen on its turret; behind it a five-
to six-storey block whose corner and roof are torn open with the floors hanging, classical two- and
three-storey buildings, and on the right a 1930s block with continuous window bands and piers, its
windows blown out; a horse cart and pedestrians.
Reproduce: the scale relationship the player will see from outside: the 3.0 m Tiger against blocks
of five to six storeys. Scaled from the Tiger's 6.3 m hull (about 43 px per metre at its depth) and
the right-hand block's base line, which meets the pavement 2.5-3.5 times further from the
horizon-to-ground distance of the Tiger, that block's storey pitch is roughly 3.0-3.7 m (+/-25 %:
the horizon itself is uncertain). The square is 100 m or more across; the dark vehicle reads darker
than the pale paving.
Use: V (primary: a Tiger in a ruined Soviet city, scale), G (1930s constructivist block, corner
damage), L (square width).

**[01-14]** `01_tiger_i/01_tiger-craned-onto-rail-wagon-henschel-kassel_barch-146-1972-064-61.jpg`
Bundesarchiv Bild 146-1972-064-61, photographer not named, 1942, the Henschel works, Kassel. Grade B
(year only).
Shows: a Tiger hanging in chains from an overhead crane above a rail wagon; the hall's steel window
walls of small panes; brick walls with segmental-arched windows; another tank on a wagon.
Reproduce: the running gear from below; industrial glazing of small panes (about 0.4-0.6 m) in steel
frames, the same construction as the Stalingrad shops ([04-15]).
Use: V (running gear), G (industrial glazing).

**[01-15]** `01_tiger_i/01_tiger-commander-in-drum-cupola_barch-101iii-zschaeckel-206-34.jpg`
Bundesarchiv Bild 101III-Zschaeckel-206-34, Friedrich Zschäckel (SS-PK), July 1943, Kursk. Grade A.
Shows: a commander in camouflage smock and headset standing in the drum cupola, its round lid raised
vertically on its hinge; kit on the turret roof; open country with vehicles.
Reproduce: the early drum cupola: vision slits round the drum, a lid that swings up to vertical; the
commander's chest clears the cupola rim when he stands.
Use: V (cupola), people.

**[01-16]** `01_tiger_i/01_tiger-interior-crewman-at-vision-block_barch-101i-022-2949-19.jpg`
Bundesarchiv Bild 101I-022-2949-19, Heinz Mittelstaedt, July 1943 (Citadel; batch date 1943-06-21);
the archive itself asks whether this is a Tiger. Grade A for the record, uncertain vehicle.
Shows: a crewman's hand at a vision block in a dark compartment; a bright horizontal vision slit; an
open hatch overhead as a bright disc; light-painted interior walls with scuffs.
Reproduce: inside a closed tank the only light comes through vision blocks and open hatches; the
light-painted interior spreads it; everything else is near black. This is exactly the physical
behaviour the renderer must produce with no fill light.
Use: V (interior light transport), S.

**[01-17]** `01_tiger_i/01_tiger-rounds-lowered-through-loader-hatch_barch-101i-022-2948-28.jpg`
Bundesarchiv Bild 101I-022-2948-28, Paul Wolff, July 1943 (Citadel). Grade A.
Shows: from above, an 8.8 cm round passed by hand down through the loader's hatch to a man inside;
the thick hatch coaming; clothing and kit on the turret roof; the interior with the round bins.
Reproduce: the hatch opening is about 0.5 x 0.6 m, tight for a man in winter clothing; a round is
about 0.9-1.0 m long (+/-0.1 m, against the hands).
Use: V (hatch, rounds), S (resupply).

**[01-18]** `01_tiger_i/01_tiger-turret-removal-rigging-gantry-crane_barch-101i-022-2922-22.jpg`
Bundesarchiv Bild 101I-022-2922-22, Kipper series (PK 637), 1943 (Kursk period; batch date). Grade
A.
Shows: three men rigging chains from a portable gantry crane's pulley block to the turret; the gun
and mantlet; spare track links on the hull front.
Reproduce: the field workshop's gantry crane and chains; the men's scale on the turret roof.
Use: M (rear-area workshop), S.

**[01-19]** `01_tiger_i/01_tiger-turret-hoisted-open-hull_barch-101i-022-2922-36.jpg`
Bundesarchiv Bild 101I-022-2922-36, Kipper series, 1943. Grade A.
Shows: the turret hanging from a steel lattice gantry with its basket below it and two men on it;
the hull open beneath, a man reaching up; flat bare ground and trees.
Reproduce: the gantry's clear height about 6-7 m (+/-15 %, from the men); the turret with its basket
is about 2.5 m tall overall; the turret ring about 1.8 m.
Use: V (turret and basket dimensions), M (workshop).

**[01-20]** `01_tiger_i/01_tiger-crew-in-open-engine-compartment_barch-101i-022-2936-27.jpg`
Bundesarchiv Bild 101I-022-2936-27, Altvater, June-July 1943. Grade A.
Shows: two crewmen sitting in the open engine compartment over the V-12's cylinder heads and fuel
lines, cleaning parts; a helmet on the deck.
Reproduce: the engine bay's depth: men sit in it to the waist.
Use: V (engine bay), S.

**[01-21]** `01_tiger_i/01_tiger-323-engine-deck-maintenance_barch-101i-022-2936-22.jpg`
Bundesarchiv Bild 101I-022-2936-22, Wolff/Altvater, July 1943 (Citadel). Grade A.
Shows: a crewman head-down through an engine-deck hatch; the mesh-covered fan grilles; the turret's
dark numerals 323 with a pale outline.
Reproduce: turret numbers as dark numerals with a pale outline about 0.35-0.45 m tall; mesh grilles
over the deck fans.
Use: V (markings, engine deck).

**[01-22]** `01_tiger_i/01_tiger-engine-exchange-with-crane-truck_barch-101i-024-3536-28.jpg`
Bundesarchiv Bild 101I-024-3536-28, Vorpahl, 1944, southern Soviet Union (Romania). Grade A.
Shows: a truck-mounted slewing crane lifting the engine out of a Tiger on open ground; men standing
on the deck and round the vehicle; tarpaulins, a bucket and a dog on the ground; a telegraph pole.
Reproduce: engine changes in the open with no building; the power pack is about 1.3 m long and 1.0 m
tall (+/-15 %, against the men).
Use: M (field repair), S.

**[01-23]** `01_tiger_i/01_tiger-engine-being-installed-factory_barch-101i-635-3965-25.jpg`
Bundesarchiv Bild 101I-635-3965-25, Hebenstreit, 1944, a German tank factory. Grade A.
Shows: the engine lowered into a bare hull on a trolley; men on the hull; a factory hall with crane
runways.
Reproduce: the hull tub's side height about 1.8 m over its floor (against the men).
Use: V (hull proportions).

**[01-24]** `01_tiger_i/01_tiger-hull-interior-from-above-factory_barch-101i-635-3966-07.jpg`
Bundesarchiv Bild 101I-635-3966-07, Hebenstreit, 1944, a tank factory. Grade A.
Shows: from above, a turretless Tiger hull with its turret-ring opening and men working inside;
stacks of road wheels and track components beside the line.
Reproduce: the fighting compartment's floor plan under the 1.8 m turret ring.
Use: V (interior volume).

**[01-25]** `01_tiger_i/01_captured-tiger-tunisia-british-evaluation_iwm-na2640.jpg`
IWM NA 2640, Sgt Loughlin (No. 2 AFPU), 6 May 1943, Tunisia. Grade A.
Shows: a captured early Tiger on a grassy hillside at high resolution: spare links across the lower
hull front, the driver's visor and the hull machine-gun mount, the drum cupola, the loader's hatch
open; on the glacis the crew's things laid out (an upright round, boxes, tins); greatcoats, tunics,
mess tins and a gas-mask canister strewn on the ground in front; a British truck; open rolling
country.
Reproduce: a Tiger abandoned or captured intact: the crew's clothing and kit strewn within 5-10 m of
the hull and small things laid on it, the same debris halo as [06-08].
Use: V (Tiger front at high resolution), S (an abandoned Tiger's surroundings).

**[01-26]** `01_tiger_i/01_captured-tiger-display-horse-guards-london_iwm-h34426.jpg`
IWM H 34426, Lt Tanner, 18 November 1943, Horse Guards Parade, London. Grade A.
Shows: a captured Tiger on public display with soldiers climbing on it and a queue of visitors.
Reproduce: human scale only: the visitors' heads, at about 1.75 m, are level with the hull roof at
the rear (so the hull roof is about 1.8-1.9 m), and the turret roof stands about 1 m higher
(+/-10 %).
Use: V (human scale against the Tiger).

**[01-27]** `01_tiger_i/01_captured-tiger-us-display_1944-05-22.jpg`
Photographer not named, 22 May 1944, a display in the United States (USAHEC). Grade B.
Shows: a captured early Tiger head-on at high resolution on a plank ramp, two men in German tropical
uniform standing on the hull front beside the turret; the front plate, visor, ball mount, two
headlights, tow shackles, front mudguards, battle tracks and interleaved wheels.
Reproduce: the front plate's details and proportions, the link pattern of the battle track, the
mudguard edges; the men on the hull front (about 1.2 m above the ground) give the turret roof at
about 2.9 m.
Use: V (primary high-resolution geometry check for the player Tiger's front).

**[01-28]** `01_tiger_i/01_soviet-tankmen-on-captured-tiger-221_1943-08.jpg`
Grigory Chertov (TASS), August 1943, Leningrad front (Mga), Tiger 221 of s.Pz.Abt. 502. Grade B.
Shows: Soviet tankmen sitting on the gun barrel and turret of a knocked-out Tiger in a forest shot
to pieces: branchless, splintered pine trunks 10-20 m high all round; the turret and hull sides
peppered with dozens of shallow hit marks; a crumpled rear mudguard and stowage bin; a trench with a
dugout and soldiers in the foreground; loose sandy spoil.
Reproduce: sustained artillery fire leaves a forest of bare, splintered trunks with no crowns; a
Tiger that has survived many hits carries dozens of shallow craters and gouges over its turret and
hull sides, as geometry and material state (compare [08-01], [08-02]).
Use: V (hit marks on the Tiger), M (shattered forest for later acts), S (a captured Tiger).

**[01-29]** `01_tiger_i/01_tiger-track-width-vs-boot_nara-111-sc-171655.jpg`
NARA 111-SC-171655, 23 April 1943, Béja, Tunisia. Grade A.
Shows: a soldier's boot on the Tiger's battle track for scale; chalk letters on the hull; gravel and
earth packed into the links.
Reproduce: the battle track is about 2.4 boot lengths wide (725 mm); earth packs into the recesses
of each link.
Use: V (track).

**[01-30]** `01_tiger_i/01_tiger-interleaved-road-wheels-wreck_nara-111-sc-171656.jpg`
NARA 111-SC-171656, Haber, 23 April 1943, Béja, Tunisia. Grade A.
Shows: a wrecked Tiger at a roadside ditch: the engine deck blown open with its plates and contents
torn up; the turret still on; the interleaved road wheels, one bent; the track broken off the
sprocket and lying in the ditch; broken ground at the hull front; a crouching US soldier; vehicles
and poles on the road.
Reproduce: a Tiger destroyed by an internal or demolition charge in the engine compartment keeps its
turret while the deck plates tear and curl; wheels bend; the track breaks and drops.
Use: V (a Tiger damage state), S (Tigers blown up where they could not be recovered).

### 2.2 Folder 02: Soviet armour (21)

**[02-01]** `02_soviet_armour/02_t34s-and-infantry-attack-kalach-snow_1942-11.jpg`
Photographer unknown (Soviet), November 1942, the Kalach area (Operation Uranus), per the Commons
description of a book scan (S. Walsh, 2000; Ukrainian Central State Archive). Grade C; [staged?].
Shows: infantry in greatcoats, fur caps and a few helmets running across thin snow toward two T-34s
with brushed-on white patches; behind them the long facade of a gutted two-storey brick building
standing alone with its window openings at a regular bay and its roof and floors gone, a gable-end
ruin, a low house with a hipped roof, a snow-covered heap; smoke on the left; weeds standing through
the snow.
Reproduce: in the settlements on the route west a single long wall with two rows of empty windows
can be all that is left of a brick building; snow is 5-15 cm with weeds and grass through it;
infantry move spread out 3-10 m apart beside and behind the tanks.
Use: M (settlements on the route west), V (T-34 winter wash), S.

**[02-02]** `02_soviet_armour/02_t34-column-riders-white-suits-little-saturn_1942-12.jpg`
Photographer unknown (Soviet), December 1942, the middle Don (Operation Little Saturn), per the
Commons description of a book scan. Grade C; [print] (heavy grain and contrast).
Shows: a T-34 against the light with riders in white hooded suits sitting on the engine deck behind
the turret, snow on the hull; blowing snow and exhaust; a second tank about 100 m behind reduced to
a grey silhouette.
Reproduce: in blowing snow against a low sun, a tank at about 100 m keeps only its outline; riders
sit behind the turret and hold on to the rails.
Use: V (backlit haze and visibility), M (the steppe in December), people.

**[02-03]** `02_soviet_armour/02_t60-with-riders-white-suits-bryansk-front_1943-01.jpg`
Aleksandr Gribovsky (TASS), January-February 1943, Bryansk Front. Grade B.
Shows: a T-60 light tank carrying about seven riders in hooded white suits, crowded on the hull and
round the small turret; snow on the running gear; a T-34 with riders far off on the left; a flat,
wind-smoothed snowfield to a hazy horizon; low sun.
Reproduce: a T-60's hull top is at about the seated riders' hips (hull about 1.4 m, the turret top
about 1.75 m); on open snowfields the surface is wind-smoothed with vehicle tracks and the horizon
is lost in haze.
Use: people (riders in white suits), V (T-60 scale), M (open snowfield).

**[02-04]** `02_soviet_armour/02_tank-riders-jumping-off-t34-caucasus_1942.jpg`
Max Alpert (TASS), 1942, Caucasus (52nd Tank Brigade). Grade B; [staged?].
Shows: helmeted riders dropping off the rear of a T-34 with a painted tactical diamond while
infantry fan out on both sides; a second T-34 ahead carrying men; a wattle fence along a maize
field; long green hills.
Reproduce: wattle fences 1.5-1.8 m high along plots and fields; riders dismount over the rear deck
and the track guards.
Use: people, M (summer and autumn acts), S.

**[02-05]** `02_soviet_armour/02_tank-riders-t34s-start-line-birch-wood_1942-05.jpg`
Mikhail Bernshtein, May 1942, South-Western Front. Grade B.
Shows: three T-34s, each carrying six to ten helmeted riders, drawn up at the edge of a thin birch
wood; infantry in greatcoats among the trees; felled birch trunks and brush on the ground.
Reproduce: assembly at a wood edge; felled trunks and brush litter; riders sit on the engine deck
and round the turret.
Use: people, M (woods on the route).

**[02-06]** `02_soviet_armour/02_tank-riders-dismounting-t34_1942.jpg`
Mikhail Bernshtein, 1942, South-Western Front. Grade B; [staged?].
Shows: helmeted riders on a T-34 with a disruptive camouflage pattern, one firing a rifle from the
turret side, others crouched ready to jump; a soldier with a backpack already running on the ground;
mud caked on the track guards; a scrub field.
Reproduce: the turret's rear overhang and the engine-deck handholds the riders use; mud caked on the
tracks and the guards.
Use: people.

**[02-07]** `02_soviet_armour/02_whitewashed-t60-riders-sapper-mines_1942.jpg`
Photographer unknown, 1942 (Commons, title "Checked. No mines"). Grade C.
Shows: a whitewashed T-60 with riders in sheepskin coats and fur caps; a sapper with a ring-type
mine detector on a pole; a row of lifted German anti-tank disc mines laid in the snow by the road; a
forest of snow-laden trees.
Reproduce: lifted mines laid in rows beside a cleared road, each disc about 0.32 m across and 0.1 m
tall; fresh snow 20-30 cm; the whitewash worn on the T-60's edges.
Use: S (a cleared road or street), L (mined streets and their clearance), people.

**[02-08]** `02_soviet_armour/02_t34-with-riders-kharkov-feb-1943_1943-02-16.jpg`
Photographer unknown, 16 February 1943, the centre of Kharkov. Grade C. Kharkov, not Stalingrad.
Shows: a 1942-43 T-34 with the hexagonal turret, both round roof hatches open, fuel drums on the
rear, carrying about ten riders across a snowy square cut by wheel and track ruts; large gutted
blocks and smoke in haze beyond; people walking far off.
Reproduce: in a city square 5-10 cm of snow is cut by ruts down to dark earth, and vehicles leave a
web of tracks; large blocks at 300-500 m fade into haze.
Use: V (T-34 of 1942-43 in a winter city), L (a square used by vehicles), people.

**[02-09]** `02_soviet_armour/02_t34-and-crews-winter-operation-gallop_1943.jpg`
Photographer unknown, February 1943, 25th Tank Corps, Operation Gallop (Commons; source
tankfront.ru). Grade C.
Shows: men in long sheepskin coats and fur hats, one kneeling at a truck tyre; a whitewashed T-34
with the wash broken into patches; trucks behind; trodden snow under hard sun.
Reproduce: the long sheepskin coat reaching the shin; hard winter sun on trodden snow, shadows
sharp.
Use: people, M.

**[02-10]** `02_soviet_armour/02_t34-crew-with-fuel-drum-winter_1942.jpg`
Photographer unknown (Soviet), winter 1942-43 (book scan; Ukrainian Central State Archive). Grade C.
Shows: two tankmen in quilted jackets and fur caps filling a bucket from a 200-litre drum beside a
log house with a straw roof; whitewashed T-34s behind; bare trees; snow on the drums.
Reproduce: 200-litre drums with rolling hoops; refuelling by bucket; log houses under straw roofs at
the settlement edge.
Use: S (resupply at a halt), M (villages), people.

**[02-11]** `02_soviet_armour/02_t34-crew-24th-tank-corps-turret-e327_1942-12.jpg`
Photographer unknown, December 1942, 24th Tank Corps (from 26 December 2nd Guards Tank Corps). Grade
C.
Shows: six tankmen in padded helmets, fur caps and sheepskin collars on a T-34 turret with a
hand-brushed white tactical number; worn paint; a raised hatch.
Reproduce: tactical numbers brushed on by hand, about 0.3-0.4 m tall, uneven; worn paint on the cast
turret.
Use: people (tank crew dress), V (markings).

**[02-12]** `02_soviet_armour/02_t34-crew-90th-brigade-univermag-jan-1943_1943-01-31.jpg`
Anton Zenin, 31 January 1943, in front of the Univermag: the crew of 90th Separate Tank Brigade.
Grade B.
Shows: four crewmen (greatcoats, sheepskin coat, a padded tank helmet with goggles, fur caps) in
front of a T-34 with a slogan painted on its turret; the Univermag's rounded corner and belvedere
behind; people crowding the square beyond; about 10-20 cm of snow.
Reproduce: the square on the last day of the fighting: snow 10-20 cm, trodden; T-34s in the square.
Slogans on turrets are attested, but readable words break the first principle: keep them illegible
at game resolution or leave them off.
Use: L and V (the Univermag square at the end of January), people (Soviet tank crews), S.

**[02-13]** `02_soviet_armour/02_kv1s-after-battle-don-front-snow_1942-12-10.jpg`
Semyon Alperin, dated 10 December 1942, 65th Army, Don Front, per Commons (the source file name says
1943, Voronezh Front; date and place uncertain). Grade C; [prop] (series title "One against five").
Content note: a dead crewman lies over the turret hatch.
Shows: a whitewashed KV-1S with its turret number; a crewman's body slumped face down over the hatch
rim, arms hanging, small in the frame; spent 76 mm cases and an ammunition box on the snow beside
the tracks; far off on a snow slope, a line of knocked-out German tanks; bare trees; hard sun.
Reproduce: a tank that fought from a halt leaves spent cases (about 0.4 m) and ammunition boxes on
the snow beside it; its victims lie 300-600 m off in a line. A crewman killed leaving the tank lies
over the hatch rim; if a tableau uses this attested condition it must be as restrained as this
frame: small, still, no wounds shown.
Use: S (with restraint), V (KV-1S in snow, spent cases).

**[02-14]** `02_soviet_armour/02_t34-column-145th-brigade-on-march_1942-05.jpg`
Dmitry Chernov (TASS), May 1942, Golitsyno (Moscow region), 145th Tank Brigade. Grade B.
Shows: the platoon commander in a padded tank helmet head-out of a T-34 turret, the big single hatch
lid of the 1941-42 turret raised behind him; a painted tactical diamond; tow cables; the column on a
dirt road between spruces.
Reproduce: the 1941-42 turret's large rear-hinged hatch; column spacing of about 15-25 m.
Use: V (T-34 turret detail), people.

**[02-15]** `02_soviet_armour/02_t34s-and-infantry-crossing-field_1942-05.jpg`
Dmitry Chernov (TASS), May-June 1942, Western Front. Grade B; [staged?].
Shows: three T-34s spread 30-60 m apart across a meadow with helmeted infantry advancing among them;
spruces and birches.
Reproduce: tank intervals of 30-60 m in the open, infantry 3-10 m apart.
Use: M (summer acts), people.

**[02-16]** `02_soviet_armour/02_t34-model-1940-in-ambush-wood_1942-05.jpg`
Mikhail Bernshtein, catalogued May 1942 (thin snow and bare trees point to winter or early spring).
Grade B for the source, date doubtful.
Shows: an early T-34 standing among the bare trunks of a deciduous wood, thin snow, fallen branches
and brush in the foreground hiding its hull.
Reproduce: a tank 30-50 m inside a wood among trunks of 0.2-0.4 m at 2-4 m spacing (+/-30 %, scaled
from the tank) is hard to see: the trunks and brush break its outline.
Use: M (woods; concealment), V.

**[02-17]** `02_soviet_armour/02_t34-mod-1942-ural-volunteer-tank-corps_rian-1274.jpg`
RIA Novosti #1274, V. Kaushanov, dated 1 February 1942 (doubtful: the Ural Volunteer Tank Corps
formed in 1943), Sverdlovsk region. Grade A for the source, date doubtful.
Shows: new, evenly whitewashed T-34s on railway flat wagons inside a hall under a steel lattice
roof; crewmen in sheepskin coats and fur hats.
Reproduce: a factory whitewash is even, unlike the field wash; T-34 running gear (five large road
wheels) at close range.
Use: V (T-34 geometry), M (railheads).

**[02-18]** `02_soviet_armour/02_captured-t70-light-tank-german-soldier_nac-2-1623.jpg`
NAC sygn. 2-1623, Momber (German PK), April 1943. Grade A.
Shows: a German soldier sitting on a captured, worn-whitewashed T-70; a tarpaulin and laundry hung
on it; the engine hatch open; bare trees; trucks; wet spring snow.
Reproduce: T-70 scale: hull about 1.4 m high, turret top about 2.0 m; captured vehicles used as
racks for tarpaulins and washing.
Use: V (T-70 geometry), S (captured vehicles in use).

**[02-19]** `02_soviet_armour/02_t60-light-tank-malyutka_1943.jpg`
Photographer unknown, 1943, T-60 "Malyutka" (Historical Archive of Omsk Oblast). Grade C; [print]
(dark, overexposed).
Shows: a T-60 from the front quarter: four road wheels, a small offset turret, a broad sloped
glacis.
Reproduce: T-60 proportions only.
Use: V (T-60).

**[02-20]** `02_soviet_armour/02_t34-colour-slide-steppe-1942_barch-169-0014.jpg`
Bundesarchiv Bild 169-0014, photographer not named, summer 1942 (print strip; date field 1942-06-21
is a batch date). Grade A-; [colour].
Shows: a T-34 with the early cast turret, apparently abandoned, on the lip of the steppe above a
wide valley; dry grass; a telegraph line along the valley floor; a hazy horizon at several
kilometres.
Reproduce: the southern steppe in summer: pale dry grass, a valley several kilometres wide,
telegraph lines; relative value: the painted tank is much darker than the dry grass, the sky lighter
still.
Use: M (the steppe in summer), V (relative value only).

**[02-21]** `02_soviet_armour/02_kv1-colour-slide-on-steppe-ridge_barch-169-0011.jpg`
Bundesarchiv Bild 169-0011, photographer not named, summer 1942 (batch date 1942-06-21). Grade A-;
[colour].
Shows: a KV-1 on a crest above a balka: long convex grass slopes, a gully with shrubs and trees on
the left, a hazy plain beyond.
Reproduce: balka relief: convex, grassy shoulders falling into shrub-filled gullies; a tank on a
crest is skylined against the pale horizon from below.
Use: M (balka relief), V.

### 2.3 Folder 03: Soviet anti-tank guns and rifles (17)

**[03-01]** `03_soviet_anti_tank/03_45mm-53k-crew-direct-fire-mozdok_1942-10.jpg`
Max Alpert (TASS), October 1942, Mozdok area (Commons, PD-Russia). Grade B; [staged?] (crew lined up
in profile for the camera).
Shows: a 45 mm 53-K firing over open sights on bare, dry ground below a long low ridge; six men
kneel or crouch in a line behind and beside the shield; the muzzle blast lifts a dust cloud wider
than the gun.
Reproduce: the gun's height against kneeling men: the shield top is at kneeling-head height, so the
whole piece is about 1.1-1.2 m tall (+/-0.1 m, from the crouched crew); disc wheel about 0.75 m. The
blast raises the ground cover (dust here, snow in winter) for several metres to both sides, which is
how a hidden gun gives itself away.
Use: V (45 mm silhouette and crew positions), S (emplacement traces: disturbed ground).

**[03-02]** `03_soviet_anti_tank/03_zis3-crew-firing-in-stalingrad-ruins_1942-11.jpg`
Photographer unknown, "November 1942, Stalingrad" (Commons, PD-Russia-2008). Grade C (no archive
record; the snow depth fits late November-January).
Shows: a ZiS-3 in the open among ruins, trails spread in trampled snow, five men in greatcoats,
balaclavas and ushankas serving it. Behind: the corner of a gutted three- to four-storey brick
building standing free, its walls broken into a stepped, ragged profile; a dark horizontal band at
first-floor level (burned floor line or plastered string course); more gutted blocks and a factory
chimney beyond; a large bent sheet of roofing iron hangs into the top right of the frame.
Reproduce: corner ruins that keep two walls to full height and lose the rest. The window openings
are about 1.6 times as tall as they are wide and the storey pitch about 1.7 times the window height;
with a storey pitch of 3.3-3.8 m that gives windows about 1.2-1.4 m wide and 2.0-2.2 m tall
(+/-20 %; the ZiS-3 in the foreground cannot scale a building tens of metres behind it). Broken
beams and planks lie on the snow; a flat open space of 50-150 m between ruins is what gives a gun
its field of fire. The gun's shield top stands about 1.4 m above the snow.
Use: G (corner-ruin failure), L (open ground between ruins, gun sites), V.

**[03-03]** `03_soviet_anti_tank/03_ptrd-team-in-stalingrad-ruins_1942-11.jpg`
Efim Kopyt (TASS), November 1942; the anti-tank rifle team of 108th Guards Rifle Regiment, 36th
Guards Rifle Division, 64th Army (southern Stalingrad). Grade B (named men and unit).
Shows: a PTRD-41 gunner and a loader with a PPSh behind a breastwork of broken brick; 14.5 mm rounds
laid out on a cloth. Behind them: two horizontal riveted steel tanks or boilers (about 1.5-2.5 m in
diameter, +/-25 %, against the kneeling men), heavy brick piers with projecting banded courses, a
steel beam frame and a bent pipe; the brick reads pale with dark joints in this print (tone only).
Reproduce: rubble is a mixture of loose bricks and bonded lumps of 3-20 bricks (0.2-0.6 m), not
uniform gravel; brick piers about 1.0-1.3 m wide (+/-0.15 m, counted in brick lengths); courses read
clearly at 77 mm; industrial yards hold large steel vessels that serve as cover.
Use: G (brick bond, piers, rubble lump sizes), L (factory-edge yard), V.

**[03-04]** `03_soviet_anti_tank/03_76mm-usv-crew-firing-on-stalingrad-street_1942-10.jpg`
Pavel Troshkin, published in Izvestia on 9 October 1942; the gun crew of N. D. Yanovsky on a
Stalingrad street. Grade B.
Shows: a 76 mm F-22 USV served by helmeted men in a dirt street of one-storey timber houses. The
house behind has walls clad in vertical boards, small windows, a gable end with a fretwork
bargeboard and a gabled roof of standing-seam sheet iron, one seam torn open at the eaves; a low
picket fence in front of it and a taller solid board fence beside the gun, with boards missing; a
telegraph pole; a birdhouse on a pole; a truck behind a fence; spent cases on the ground.
Reproduce (wooden_house): one storey; eaves about 3.0-3.4 m and ridge about 5.0-5.5 m above ground
(+/-20 %: the camera stood about 1.2-1.3 m up, since the kneeling crew's heads sit on the horizon,
and heights at the house's depth are read against that horizon); roof pitch about 30 degrees;
frontage at least 12 m; picket fence 0.8-1.1 m, board fence 1.6-2.0 m; gaps where boards have been
pulled or shot away. The street is unpaved earth.
Use: G (wooden_house form, roof, fences), L (fringe streets, plot fences), V.

**[03-05]** `03_soviet_anti_tank/03_76mm-usv-antitank-battery-near-stalingrad_1942-08.jpg`
Emmanuil Evzerikhin (TASS), August 1942, the battery of Sr. Lt. Ye. P. Goltsov, 99th Tank Brigade,
near Stalingrad. Grade B.
Shows: a USV divisional gun screened with cut oak branches on a grassy slope at a wood edge, loaders
carrying rounds, smoke drifting.
Reproduce: guns camouflaged with cut branches laid over the shield and barrel (summer practice); the
crew spreads 3-6 m behind the trails.
Use: M (steppe-edge battery sites in the pre-winter acts).

**[03-06]** `03_soviet_anti_tank/03_zis3-crew-firing-north-caucasus_1943-06.jpg`
Max Alpert (TASS), June 1943, North Caucasus. Grade B; [print] (sky burned in dark, heavy grain).
Shows: a ZiS-3 crew firing from a slope, a loader running with a round, dust around the wheels.
Reproduce: crew postures in action only; the tonal extremes are darkroom work, not light.
Use: people posture only.

**[03-07]** `03_soviet_anti_tank/03_camouflaged-zis3-firing-kursk_1943-07-06.jpg`
Pavel Troshkin, 6 July 1943, Kursk salient (Orel-Kursk direction). Grade B; [staged?] [print]
(grainy, high-contrast print).
Shows: a ZiS-3 sited low between oak bushes, its shield top level with the scrub; the gunner at the
sight, a loader kneeling with a 76 mm round, the commander with binoculars in the foreground,
another man with a round on the right; a shallow pit behind a parapet of earth and cut grass; flat
open country beyond to a horizon with smoke.
Reproduce: a gun position is a shallow pit (0.3-0.5 m) behind a low parapet, with the shield top at
the height of the surrounding scrub (about 1.2-1.5 m) so that only the barrel shows above the cover;
the field of fire is open ground of 1-2 km.
Use: M (summer and autumn acts), S (anti-tank gun emplacement), V (crew posture at a gun).

**[03-08]** `03_soviet_anti_tank/03_45mm-crew-white-suits-winter-1941_1941-12.jpg`
Sergey Strunnikov, December 1941, Western Front: the crew of Jr Sgt A. Minaev. Grade B; [staged?]
(posed close-up).
Shows: a 45 mm 53-K with wire-spoked wheels on pneumatic tyres, snow caked on the shield's top edge,
the tyres and the barrel; three crewmen in hooded white oversuits behind the shield; a limber or
ammunition cart behind; a snow bank with dark earth showing on the left.
Reproduce: snow clings to the upper edges of the gun and to the tyre tops, and the shield's lower
half stays dark; the 45 mm wheel is about 0.8 m and the shield top about 1.0-1.1 m above the ground
(+/-0.1 m, from the crouching crew).
Use: V (45 mm gun in winter), people (white suits).

**[03-09]** `03_soviet_anti_tank/03_45mm-guns-rolled-out-of-pillbox-pulkovo_1942-03.jpg`
Boris Losin (TASS), March 1942, Pulkovo (Leningrad front). Grade B; [staged?] (a demonstration for
the camera).
Shows: two 45 mm guns with whitewashed shields pushed out of a log-lined gun bunker by crews in
hooded white oversuits; the bunker is a snow-covered earth mound about 2.5-3 m high with a wide
opening framed by horizontal logs; a birch wood and a five-storey building behind; hard low sun and
long shadows on wind-rippled snow.
Reproduce: a gun bunker is a log chamber about 4-5 m wide and 1.8-2.0 m high inside under an earth
mound 1-1.5 m thick, whose snow cover follows its rounded shape; the opening faces the field of
fire; gun shields painted white in winter.
Use: M and L (anti-tank bunkers on defence lines at a district edge), S, V (sun on snow, long
shadows).

**[03-10]** `03_soviet_anti_tank/03_45mm-53k-crew-village-edge-snow-1941_1941-11-27.jpg`
O. Knorring and A. Kapustyansky, 27 November 1941 (published in Krasnaya Zvezda, 28 November 1941),
a village near Moscow (Glavarkhiv, CC BY 4.0). Grade B.
Shows: a 45 mm 53-K with spoked wheels sited at a village edge behind a low snow-and-earth parapet;
five men in greatcoats and ushankas kneeling round it; weed stalks 0.5-1 m standing through thin
snow; a birch; a long, low house and a barn with snow on the roofs; a dark conifer forest on the
horizon; soft overcast light.
Reproduce: guns are sited at the last garden or field edge of a settlement, behind a parapet of
0.3-0.5 m; 10-20 cm of snow with weeds standing through it; the gun's shield top is at the kneeling
crew's shoulder.
Use: M (village edges on the route west), L (the district's fringe), V.

**[03-11]** `03_soviet_anti_tank/03_45mm-53k-crew-in-position-ponyri_1943-07.jpg`
Pavel Troshkin, July 1943, Ponyri (Kursk). Grade B; [staged?].
Shows: four crewmen in side caps kneeling tight behind the shield of a 45 mm gun in tall grass, the
commander with binoculars; bundles of grass tied to the wheel and shield; rounds in hand; trees and
a long view over open country.
Reproduce: summer camouflage is grass tied to the gun itself; the kneeling crew's heads are at the
shield top (about 1.2 m); the shield's shaped upper edge is the silhouette to reproduce.
Use: M (summer acts), people, V (45 mm silhouette).

**[03-12]** `03_soviet_anti_tank/03_45mm-crew-pushing-gun-to-open-position_1943-05.jpg`
A. Zverev (TASS), May 1943: Sgt Popov's crew moving the gun to an open position for direct fire.
Grade B.
Shows: about eight helmeted men haul a 45 mm gun up the bank of a ditch by the wheels and trail;
meltwater and the last snow in the bottom; a flat field and a far tree line.
Reproduce: without a tractor a 45 mm gun needs six to eight men to cross a ditch bank of 1-1.5 m; in
the thaw, snow survives only in hollows and along water.
Use: M (spring and thaw acts), S (a gun moved by hand).

**[03-13]** `03_soviet_anti_tank/03_ptrd-team-firing-in-snow-winter-1942_1942-03.jpg`
Kazimir Lishko (TASS), March 1942 (published 13 March 1942): Sgt I. I. Karnaukhov and P. M.
Malakhov, 99th Guards Cavalry Regiment. Grade B; [staged?] (the burst is conveniently placed).
Shows: two men in hooded white suits lying prone on a snow slope behind a PTRD whose barrel is
wrapped in white cloth; spent cases on the snow; a dark shell burst on the slope behind; a valley,
scrub and a village of low houses on the far ridge.
Reproduce: a burst on snow throws a dark column of earth and snow roughly 5-10 m high (+/-50 %, no
reference stands at its distance) and leaves a dark fan on the white; the PTRD is about 2 m long on
its bipod; a village skyline at 1-2 km.
Use: V (burst on snow), people, M.

**[03-14]** `03_soviet_anti_tank/03_ptrd-team-snow-trench-moscow_1941.jpg`
Photographer unknown, winter 1941-42, Moscow region (Commons; source the Zelenograd museum of local
history). Grade C.
Shows: three men in ushankas in a snow-covered trench, one aiming a PTRD over the lip, one with a
rifle, a third beside a dugout opening; a crate and a helmet-shaped lump on the parapet; a picket
fence and a vehicle wreck behind.
Reproduce: the trench is about 1 m deep, so the men's shoulders are at the lip; snow rounds the lip,
with dark earth showing where it was dug; the dugout opens off the trench wall.
Use: S (a manned position), L (trenches through yards and gardens), people.

**[03-15]** `03_soviet_anti_tank/03_ptr-teams-sudarov-likhanov-firing-position_1942.jpg`
Kazimir Lishko (TASS), 1942, South-Western Front: the anti-tank rifle crews of S. F. Sudarov and N.
K. Likhanov, 989th Rifle Regiment, 226th Rifle Division. Grade B; [staged?].
Shows: two anti-tank rifle teams and a rifleman prone in snow among frosted shrub stems, in hooded
white suits; the rifles on bipods, barrels partly wrapped; hoarfrost on every twig.
Reproduce: hoarfrost on thin stems; snow 20-40 cm, deep enough to hide a prone man to the shoulders;
teams 2-4 m apart.
Use: people, M (steppe scrub in winter), V (hoarfrost).

**[03-16]** `03_soviet_anti_tank/03_red-army-soldier-with-ptrd41-closeup_1942.jpg`
Boris Ignatovich, 1942, Kalinin Front (Glavarkhiv Moskvy, CC BY 4.0). Grade B; [staged?].
Shows: a soldier in an ushanka with the star badge and a quilted jacket aiming a PTRD; a row of
14.5 mm cartridges laid on a plank; straw behind.
Reproduce: the quilted jacket's vertical channels about 5 cm apart; 14.5 mm cartridges about 15.5 cm
long laid ready on a board at the firing position.
Use: people, S (the traces of a firing position: cartridges laid out, spent cases).

**[03-17]** `03_soviet_anti_tank/03_antitank-riflemen-moving-up-caucasus_1942.jpg`
Max Alpert (TASS), 1942, North Caucasus. Grade B; [staged?].
Shows: a file of about fifteen soldiers in greatcoats and ushankas, anti-tank rifles on their
shoulders, walking down a stony path past a tall dry-stone tower; officers on a flat roof; bright
sun.
Reproduce: the anti-tank rifle is carried on the shoulder, muzzle up, by one man; the setting is
mountain architecture and does not belong in the Stalingrad maps.
Use: people only.

### 2.4 Folder 04: Stalingrad city (28)

**[04-01]** `04_stalingrad_city/04_city-centre-after-liberation-zelma_rian-602161.jpg`
RIA Novosti #602161, Georgiy Zelma, 2 February 1943, the centre of Stalingrad. Grade A. Project
board S2.
Shows: six Soviet soldiers in hooded white oversuits, rifles slung, walk across a wide trodden
snowfield. The Volga lies beyond and below as a pale band with the flat far bank on the horizon.
Left: a gutted four- to five-storey classical block with pilasters and a broken parapet, fading into
haze. Middle: two free-standing brick stacks side by side and a third alone, each about 1.0-1.3 m
wide. Right: a gutted three-storey corner block and a two-storey wing; every window shows sky, and a
string course marks each floor line. A small two-storey house with a hipped roof survives beyond.
Foreground: snow-rounded rubble mounds, a bonded brick wall stub about 1 m high, a charred tree
stump with broken limbs, fence posts and broken poles, a sagging wire, sheet metal and planks.
Reproduce: the centre is a snowfield with isolated standing masses, not a continuous street wall.
Flattened buildings are low snow mounds 0.3-1.5 m high; survivors are perimeter walls with every
floor and roof gone. The free-standing stacks are about 6-8 m tall (+/-30 %: their bases are hidden
and the ground falls away; scaled from the soldiers at about a third of their distance). Windows in
the gutted corner block are about 1.3-1.5 m wide and 2.0 m tall at a storey pitch of 3.5-4 m
(+/-20 %, from the openings' proportions). Snow is shallow (5-15 cm): brick edges and debris poke
through and boots sink only to the ankle. Buildings at 200-400 m lose most of their contrast; the
far bank at 2-3 km is barely darker than the sky. Light: bright, even overcast with the sun behind
thin cloud; no hard shadows.
Use: L (open centre with isolated ruins, ground debris), G (gutted perimeter shell, free-standing
stacks), V (primary comparison for the district overview and for distance haze), S (the city after
the fighting).

**[04-02]** `04_stalingrad_city/04_snow-field-of-ruins-from-shattered-wall_rian-2251.jpg`
RIA Novosti #2251, Georgiy Zelma, 23 December 1942, Stalingrad; caption "Destroyed Stalingrad does
not give up!". Grade A; [prop] (caption only; the view itself is plain).
Shows: an elevated view from the upper floor of a ruin, framed on the left by a shattered brick wall
and on the right by an out-of-focus brick pier. The wall has one large breach and several small
holes; two steel I-beams, the joists of the photographer's floor, project from its foot. Beyond: a
flat snow plain of 150-300 m with weed stalks, scattered debris and a line of rails; then a field of
roofless one-storey houses with door and window holes, one roof collapsed with its rafters folded
into the house, bare broken trees, poles and one walking soldier; towers and stacks fade into haze
at the horizon.
Reproduce: breach edges in brick walls follow the bond: toothed and stepped along the courses, never
smooth or round. The large breach is about 1.5 m tall and 1.6 m wide, the small holes 0.2-0.4 m
(measured from about 8.5 px per course at 77-80 mm and 19 px per stretcher, +/-20 %). Steel floor
joists outlive their floors and project 1-2 m from the wall. The one-storey quarter becomes a field
of roofless boxes; a few roofs fall in as a V of rafters. Trees are broken, not burned. Snow lies on
every ledge and on the wall's broken top.
Use: G (breach shape, steel joists, one-storey house failure), L (edge of the centre: open ground,
then a low-house quarter), V.

**[04-03]** `04_stalingrad_city/04_red-october-hall-twisted-trusses-snow_rian-2383.jpg`
RIA Novosti #2383, Georgiy Zelma, 21 January 1943, Krasny Oktyabr steelworks. Grade A.
Shows: the inside of a steel-framed shop. The roof cladding is gone; its lattice trusses and purlins
are twisted and sag between the columns into a web of bent members. A crane runway girder still
stands on its columns along the right. Snow lies on everything inside. Heavy plant in the
foreground: a bolted press or mill housing on the right, a steel platform with a ladder and a
painted sign on the left, a flywheel about 2 m across, plate and beam scrap on the floor.
Reproduce: steel halls fail roof first: cladding and purlins go, trusses deform and sag between
supports, while columns and crane girders stand; the steel is not burned black. Snow falls through
the open roof and lies 10-20 cm deep on the floor and on every horizontal machine surface. Painted
signs belong to the object but must stay illegible (this one reads clearly).
Use: M (north factory district, later acts), G (steel hall failure), V.

**[04-04]** `04_stalingrad_city/04_soldiers-through-ruined-street-canyon_rian-2391.jpg`
RIA Novosti #2391, Georgiy Zelma, dated 1 November 1942, Stalingrad. Grade A for the source; the day
may be a filing date (the snow is continuous and fresh).
Shows: a vertical frame. Three Soviet soldiers in greatcoats and fur caps climb a snow-covered
rubble slope between gutted brick buildings of four to five storeys. The right-hand wall has lost
its render: bare brick pitted by fragments at 1.5-3 m, a ragged breach at ground level, and a
street-lamp bracket with its lamp still fixed at about 4 m, trailing cables. Behind, a free-standing
brick pier carries the stubs of its floor slabs at every storey, with bent reinforcing rods at the
top; the windows of the building beyond show sky. A pole leans at about 70 degrees; loose wires
cross the frame; barbed wire on a stake in the foreground; black curled roofing sheets stick out of
the snow.
Reproduce: storey pitch about 3.3-3.8 m (+/-15 %, from the soldiers at about the pier's depth).
Floors fail but leave slab or beam stubs 0.2-0.5 m deep on the walls at every level, read as
horizontal ledges on the inner faces; reinforcing bars bend over at broken tops. Rubble slopes rise
1.5-3 m against the walls. Overhead wires sag across the street; roofing sheet curls and lies on top
of the rubble.
Use: G (floor stubs, bare brick after render loss), L (narrow street or yard between ruins, wires,
barbed wire), V.

**[04-05]** `04_stalingrad_city/04_gerhardt-mill-shell-scarred-facade_rian-2227.jpg`
RIA Novosti #2227, Georgiy Zelma, 1943 (year only; no snow and hard sun point to spring or summer
1943), the Gerhardt (Grudinin) mill near 9 January Square. Grade B (identity certain).
Shows: the mill's street facade: four storeys and an attic row of small windows, seven or eight
bays, segmental brick window heads. The whole face is cratered by shell and bullet strikes and
spalled in large patches, so the openings have rounded edges. A two-bay breach at the fourth floor
shows the internal posts and beams standing. Several lower windows keep their frames and glazing
bars. A broken corbelled cornice; a chimney stub; a timber pole with wires and a hanging lamp in
front; a board fence on the right.
Reproduce: brick walls take small-arms and fragment fire as dense pitting: craters 5-30 cm, many per
square metre, with the outer half-brick spalled in patches of 1-3 m, concentrated round the
openings, whose edges round off; piers between windows thin but stand. Measured against the brick
courses (about 6.5-9 px per course, +/-20 %): windows about 1.1-1.3 m wide and 1.9-2.2 m tall; the
window height is about 1.8 times its width and the storey pitch about 2.5 times the window width, so
the pitch is about 3.0-3.6 m. A breached storey shows its internal frame still standing.
Use: G (brick facade damage, pitting density, breach), V (pockmark density comparison), L (pole,
wires, board fence), S (the surviving landmark).

**[04-06]** `04_stalingrad_city/04_grain-elevator-landmark-feb-1943_1943-02.jpg`
Sergey Strunnikov, February 1943, the grain elevator (southern district). Grade B.
Shows: the concrete working house (about ten window rows) and a lower silo gallery to its left,
holed by shells at mid-height. In front: a long one-storey brick building with a castellated parapet
and arched doorways; a tall square brick tower; two round brick stacks with broken tops and a square
stack; low sheds and a gantry. Foreground: a sunken brick-walled pit (a cellar or basin) and thin,
wind-scoured snow with bare ground in patches.
Reproduce: the elevator keeps its outline: concrete fails as local holes and gutted windows, not
collapse. The outbuildings are one storey (doorways about 2.5-3 m, +/-25 %); the working house
stands roughly four to eight times their height (no reliable absolute scale: its depth behind them
is unknown). Snow in February is thin and wind-packed, with bare dark patches.
Use: L and V (landmark silhouette for the Act 1 elevator at (900, 1100)), G (concrete landmark
damage), M.

**[04-07]** `04_stalingrad_city/04_panorama-flattened-quarters-to-volga_1943.jpg`
Sergey Strunnikov, 1943, Stalingrad (year only; thin snow with bare patches points to February or
March). Grade C.
Shows: an elevated view over a flattened residential quarter toward the Volga: rows of plots whose
houses are reduced to low wall outlines, stove stumps and chimney stubs along a regular street grid;
railway tracks crossing diagonally; beyond, gutted two-storey railway or industrial buildings with
regular window rows, roofless; the Volga, wide and pale, and the flat far bank in haze.
Reproduce: burned one-storey quarters keep their plan: plot outlines and foundations of roughly
8-12 m, lines of chimney stubs, and streets legible as clear strips between them; very few walls
stand above 2 m. Rail corridors cut through the fabric as broad cleared bands of several tracks.
Use: L (district fringe, plot pitch), G (burned wooden_house), M (route west).

**[04-08]** `04_stalingrad_city/04_tractor-factory-ruins-aerial_1943.jpg`
Sergey Strunnikov, 1943, from an aircraft (its wing is in frame), the Dzerzhinsky Tractor Factory.
Grade C (year only; identity from the caption and the layout).
Shows: long shop halls whose roofs are reduced to parallel ridges of framing with the cladding gone;
perimeter bays with window bands still standing; curving rail sidings; cratered yards; the workers'
settlement of gutted three- to five-storey blocks in parallel rows beyond; the Volga and its flat
far bank; thin, patchy snow.
Reproduce: from above, burned halls read as combs of parallel roof framing inside standing perimeter
walls; settlement blocks stand in regular parallel rows.
Use: M (north factory district layout), L (row pattern of 1930s blocks).

**[04-09]** `04_stalingrad_city/04_gutted-apartment-block-through-arch_1943.jpg`
Sergey Strunnikov, 1943, Stalingrad. Grade C.
Shows: framed by a brick archway with a wrought-iron gate leaf, a gutted four-storey rendered brick
apartment block: a regular grid of rectangular windows; floors and roof gone; interior cross walls
and chimney breasts standing; at the centre a projecting stair bay whose front wall has fallen,
leaving its concrete landings and zigzag flights standing in the open at every level; rows of small,
evenly spaced holes along the floor lines (probably the pockets of burned floor beams; possibly
vents); some windows blown out into round holes; render missing in patches; fragment pitting over
the whole wall; a rubble heap of 1-2 m along the base; no snow.
Reproduce: 1930s blocks burn and are shelled inside a standing shell: the facade walls stand to
eaves height while timber floors and the roof fall into the cellar and ground floor; the concrete
stair (landings and flights) survives and stays climbable-looking; cross walls remain as fins.
Measured in the enlarged frontal part: window height about 0.44 of the storey pitch and width about
0.40, window axes about 0.8-0.95 of the pitch apart; at a storey pitch of 3.3-3.6 m that is windows
about 1.3-1.45 m wide and 1.45-1.6 m tall on axes 2.7-3.4 m apart (+/-15 %; the pitch itself is not
measured here). Yards are entered through brick arches with iron gates.
Use: G (apartment_block, burned and shelled; gateway), V (gutted block comparison).

**[04-10]** `04_stalingrad_city/04_univermag-paulus-hq-square_1943.jpg`
Sergey Strunnikov, 1943, the Square of Fallen Fighters. Grade C for the date (year only); the
building's identity is certain.
Shows: the Univermag, the central department store in whose basement Paulus had his last
headquarters: a four-storey rendered block with a rounded corner under a rotunda-like belvedere, a
balustraded parapet, tall ground-floor shop openings and regular upper windows. Every opening is
empty and the interior dark, but the walls stand to full height. Gutted blocks of four to six
storeys close the square. Foreground: the square's garden, its trees stripped to trunks and main
limbs; a low garden wall of brick piers with pipe railings; fallen pipes; trench spoil mounds; thin
wind-scoured snow with bare ground; a hand-painted slogan board on a pole.
Reproduce: the square is a garden cut by trenches: spoil ridges 0.5-1 m, garden walls 0.6-1.0 m with
piers, trees 10-15 m reduced to trunks and main limbs, thin snow. Ground-floor shop openings are
about twice the height of the upper windows. The slogan board is attested dressing, but its text
must not be readable in game.
Use: L and V (the central square), G (large rendered civic block as a gutted shell), S (the building
where the battle ended).

**[04-11]** `04_stalingrad_city/04_people-at-fire-among-ruins-jan-1943_1943-01-31.jpg`
Semyon Alperin, 31 January 1943, Stalingrad (Commons title; no archive record). Grade B.
Shows: about twenty Soviet soldiers in greatcoats and ushankas, two in white suits, standing and
squatting round a small fire and cooking pots in the lee of the bare-brick end wall of a five-storey
1930s block. The wall carries a cantilevered concrete balcony with a steel railing, a stepped breach
at the ground-floor corner, scattered pockmarks and dark smoke streaks. Across an open trodden
snowfield stands a gutted four-storey block: an end wall with balconies and a round shell hole, a
facade whose every window shows sky, and brick chimney stacks above the roofline. On the snow: a
boot, rags, wire, planks, ice lumps, a shallow crater.
Reproduce: people gather in the lee of a big wall: a fire on the ground, pots, crates as seats. The
first balcony's floor is 4.0-4.6 m above the snow, measured from the brick courses (about 9.5 px per
77 mm course at the wall, so 8.1 mm per pixel); the men standing a few metres in front of the wall
agree within 15 %. The ground floor is therefore raised about 1 m over a cellar. The storey pitch is
at least 3.0 m (the balcony door is at least 2.4 m tall; the next balcony is out of frame). Bare
face brick in regular stretcher bond with fine joints; breach edges stepped along the bond; chimney
stacks stand 2-3 m above the wall tops once the roof has burned. The far block's end wall carries
X-shaped anchor plates of iron tie rods at floor levels, and a cloudy smoke stain rises from its
large breach rather than from each window. Snow 5-10 cm, trodden to a crust, with dark debris lying
on top.
Use: S (a fire in a wall's lee), G (apartment_block brick, balconies, stacks), L (open ground
between blocks), V (exposure: dark greatcoats against bright snow).

**[04-12]** `04_stalingrad_city/04_fallen-fighters-square-standing-ruin_1943-02.jpg`
Aleksandr Fridlyansky (TASS), February 1943, the Square of Fallen Fighters. Grade B.
Shows: a single slender wall fragment, the corner of a five-storey building with one column of
window openings over a ground-floor doorway, rising from a snow plain of rubble mounds with the sun
behind it in haze. To its right, a reinforced-concrete ground-floor frame (two columns and a beam)
with a broken member hanging to the ground; on the left lower wall remnants; on the right the
rounded corner of a large building on the square; two small figures.
Reproduce: some fragments stand at full height with nothing beside them: this one about 15-20 m tall
and 3-4 m wide (+/-25 %, from the figure at left near its depth), with a ground-floor opening about
2.5 m wide. Ground-floor concrete frames outlive their walls. Snow-covered rubble mounds of 1-2 m
with rods and pipes sticking out; backlit smoke haze with the sun a pale disc.
Use: G (tall fragment, RC frame), L and V (the square's silhouette), S.

**[04-13]** `04_stalingrad_city/04_inside-destroyed-railway-station_1943.jpg`
Sergey Strunnikov, 1943, the main railway station. Grade C.
Shows: the station hall: fluted columns with capitals carrying steel beams; the roof and ceiling
torn open; large sheets of roofing iron hanging curled from the beams, others folded on the floor;
plaster stripped in patches; doors and windows open to daylight; a floor of dust, sand or thin snow;
lamp hooks.
Reproduce: sheet-metal roofs fail by tearing and curling into flaps of 2-4 m that hang from the
structure or fold on the floor, while columns and beams stand. Columns about 6-7 m (+/-20 %, against
door openings of about 3 m). Hard sun through the open roof lays full-sun patches on the floor
inside.
Use: G (roof iron failure), L and V (the station interior; Act 1 main station at (-900, -300)).

**[04-14]** `04_stalingrad_city/04_railway-station-yard-wheelsets_nac-2-1750.jpg`
NAC sygn. 2-1750, Jesse (German PK), October 1942, Stalingrad railway station. Grade A.
Shows: rows of wagon wheelsets on the tracks; broken sleepers and rails; wrecked timber sheds with
collapsed plank and sheet roofs and bare rafters; tank wagons, flat wagons and a water crane;
telegraph poles with two to four crossarms; a gutted four- to five-storey block and a tall pale
building beyond; dry grass and dust, no snow.
Reproduce: wheelsets of 1.0-1.05 m in rows; poles about 7-9 m (+/-20 %) carrying several crossarms;
sheds of timber posts with sheet or board roofs caving inward; sleepers flush with the dirt.
Use: L (railway edge of the district, poles), M.

**[04-15]** `04_stalingrad_city/04_tractor-factory-machines-before-ruined-halls_nac-2-1762.jpg`
NAC sygn. 2-1762, Herber (German PK), November 1942, the Dzerzhinsky Tractor Factory. Grade A.
Shows: dozens of machine tools standing in the open before gutted halls; steel-framed industrial
windows with most glass gone and some sashes hanging open; brick hall walls; collapsed roof trusses;
a sheet-steel structure with three tall cyclone stacks under conical caps; a lattice crane girder
and a flat wagon in the foreground; German soldiers among the machines; no snow.
Reproduce: industrial window walls are steel grids of small panes in bands 3-5 m tall: the glass
goes, the grid stays; machine tools 1.5-2.5 m tall give waist-to-head cover in yards.
Use: M (factory district), G (industrial glazing), L (factory-edge yard).

**[04-16]** `04_stalingrad_city/04_bombed-industrial-plant-oblique-aerial_barch-146-1978-093-03.jpg`
Bundesarchiv Bild 146-1978-093-03, Niermann, 1942, oblique aerial, Stalingrad. Grade B (year only).
Shows: a long factory building whose roof has burned through along the middle, leaving its edges and
the perimeter walls with two storeys of windows; a neighbouring hall roofless with trusses in place;
craters, rubble fields and rail spurs; a small fire.
Reproduce: from above, roof failure follows the fire along the ridge; perimeter walls remain.
Use: M (factory district from the air).

**[04-17]** `04_stalingrad_city/04_smg-gunners-in-red-october-ruins_barch-183-r74189.jpg`
Bundesarchiv Bild 183-R74189 (ADN copy of a Soviet photograph by Georgy Zelma), October 1942, Krasny
Oktyabr works. Grade A for the record; [staged?] [print] (the figures are heavily retouched).
Shows: Soviet soldiers with PPSh submachine guns and a stick grenade among brick rubble inside a
ruined building: brick pilasters, a circular wall medallion, plaster densely pitted by bullets and
fragments; a fallen pipe; smoke.
Reproduce: interior plaster pitting: holes of 2-10 cm, dozens per square metre where fire
concentrated, plaster flaking round them; rubble floors of broken brick and lumps of 0.1-0.4 m.
Use: G (interior damage), S (fighting positions indoors), V.

**[04-18]** `04_stalingrad_city/04_fight-inside-red-october-shop-dec-1942_1942-12.jpg`
Photographer unknown, December 1942, a shop of the Krasny Oktyabr works (Commons, PD-Russia-2008).
Grade C; [staged?].
Shows: inside a steel-framed shop with built-up columns, a crane bridge and girders overhead and the
roof open, two soldiers in greatcoats run across a snow-covered floor strewn with twisted beams and
scrap; wrecked vehicles and machines; dense white smoke.
Reproduce: against the soldiers (1.75 m) the columns are about 1 m wide and the crane girders about
8-10 m up (+/-25 %); snow 5-15 cm on the floor under the open roof.
Use: M, V (steel hall interior with smoke).

**[04-19]** `04_stalingrad_city/04_red-army-men-in-ruined-workshop_1942-11.jpg`
Yefim Kopyt (TASS), November 1942, Stalingrad (Commons). Grade B; [staged?] (the action is composed
for the camera).
Shows: the side wall of a long workshop reduced to tall pale-brick piers carrying a steel beam, with
low sill walls between; one soldier fires a rifle over a sill wall; others run along a narrow-gauge
track; a hedgehog of crossed steel beams or rails in the foreground; at the far end a gable wall
with a segmental (arched) roof profile; a lower gabled building beyond; wet ground, no snow.
Reproduce: piers about 1.0-1.3 m wide (four to five bricks), bays about 2.5-3.5 m, piers 6-8 m high
(+/-20 %, from courses and the men); sill walls about 1.0-1.2 m, high enough that a standing man
fires over them; improvised obstacles of crossed rails and I-beams.
Use: G (workshop typology: pier and bay, and how it fails), L (obstacles), S.

**[04-20]** `04_stalingrad_city/04_cellar-command-post-red-october_1942-12.jpg`
Photographer unknown, December 1942: the command post of 178th Artillery Regiment (45th Rifle
Division) in the basement of the calibration shop, Krasny Oktyabr (Commons, PD-Russia-2008). Grade C
(anonymous; unit and place specific); [staged?] (posed and flash-lit).
Shows: four officers in sheepskin coats, quilted jackets and fur caps round a table with a white
cloth and a map; a lamp made from a shell case with a flame on top; a field telephone; cables along
the floor; a tarpaulin stretched under the low ceiling; rough dark walls.
Reproduce: the shell-case lamp as the room's emitter; tarpaulin under the ceiling to catch dust;
cables along the floor; a table of boards or crates; headroom about 2.0-2.4 m (+/-0.2 m, the ceiling
cloth close above the standing men). The flash lighting of the print is not the room's light:
physically the cellar is dark except near the flame.
Use: S (cellar command post), G (cellar headroom).

**[04-21]** `04_stalingrad_city/04_frozen-volga-bank-boat-in-snow_1943.jpg`
Sergey Strunnikov, 1943, the Volga bank. Grade C.
Shows: the steep bank rising about 20-30 m (+/-30 %, from a man standing at its foot), dark where
the snow has blown off, with a row of plank-framed dugout doorways and spoil heaps along its foot; a
line of barbed-wire posts on the shore flat; poles on the crest; the river's edge piled with broken
ice slabs; a wooden rowing boat frozen in and full of snow; a flat barge or pontoon.
Reproduce: dugout entrances 1.8-2.0 m tall with plank frames at the foot of the bank and spoil fans
in front; ice ridges of 0.5-1.5 m; wire posts about 3-4 m apart (+/-30 %).
Use: M (the Volga bank), S (dugout lines), L.

**[04-22]** `04_stalingrad_city/04_dugouts-cut-into-volga-bank_1943.jpg`
Sergey Strunnikov, 1943, the Volga bank (caption: German fortifications). Grade C.
Shows: dugout doorways with plank frames and small plank canopies cut into a steep snow slope; a
larger shelter under a snow-covered sheet-iron and plank roof; downslope, dark spoil with planks,
crates, a sheet-metal trough, iron bed frames and wire; fence posts along the slope; a flat snowy
valley floor with grass.
Reproduce: slope 30-35 degrees; doorways about 1.2 m wide and 1.6 m tall; a dark spoil fan below
each entrance, through the snow; bedsteads and furniture from the city carried into dugouts.
Use: S (abandoned positions), M (bank and balka dugout lines).

**[04-23]** `04_stalingrad_city/04_former-sadovaya-street-signpost_1943.jpg`
Sergey Strunnikov, 1943, "the former Sadovaya street". Grade C.
Shows: a painted arrow board and a triangular board nailed to a crooked pole, the only sign that
this was a street; a wide snowfield with a trodden path and a road darkened by traffic; two trucks
and a few people; long one-storey houses with low roofs beyond; the cut stems of garden shrubs; a
gutted two- or three-storey building on the right.
Reproduce: where a timber quarter has burned, the street survives only as a track across open snow;
garden shrubs broken to 0.5-1.5 m; roads worn to brown slush by traffic; improvised signs on poles,
whose text must not be readable in game.
Use: L (fringe streets), M (route west), S (the absence of the street).

**[04-24]** `04_stalingrad_city/04_barrikady-plant-ruin-feb-1943_1943-02.jpg`
Photographer unknown, February 1943: soldiers of 95th Rifle Division after the Barrikady plant was
retaken (Commons, PD-Russia-2008). Grade C; [prop] (a posed celebration); a wide, low-resolution
crop.
Shows: a crowd of soldiers raising their fur caps; behind them a four- to five-storey brick shop
block with its window grid shattered and floors hanging at the openings, a tall factory chimney,
smoke.
Reproduce: multi-storey factory blocks break at the openings, their floors hanging; the chimney
stands.
Use: M (north factory district), people.

**[04-25]** `04_stalingrad_city/04_scouts-white-suits-red-october-wreckage_1943.jpg`
Photographer unknown, "1943", reconnaissance group of 39th Guards Rifle Division, Krasny Oktyabr
works (Commons, PD-Russia-2008). Grade C (year only; snow and white suits fit January 1943).
Shows: scouts in hooded white oversuits walk away from the camera through knee-deep snow in the
steelworks yard: a frozen steam locomotive, a snow-covered wagon body, overturned wheelsets and
axles, bent beams and rails sticking out of the snow; roofless brick shop walls, a gantry frame and
a factory chimney on the horizon.
Reproduce: snow rounds every object into soft mounds; wheelsets (1.0 m wheels) buried to about a
third of their height, so 0.3-0.4 m of snow over the yard (+/-0.1 m); footpaths trodden to mid-calf.
Distant walls at 100-200 m lose most of their contrast under overcast light: the haze is visible at
district scale, so the participating medium must be dense enough to show it.
Use: L (factory-yard dressing, snow depth), V (overcast value range, distance haze), M.

**[04-26]** `04_stalingrad_city/04_tsaritsa-gorge-ruined-villa_barch-183-b22800.jpg`
Bundesarchiv Bild 183-B22800, Heine, November 1942 (print strip), Tsaritsa gorge, Stalingrad. Grade
A; [prop] (Scherl/PK caption about "the German castle").
Shows: a turreted two- and three-storey rendered villa on the rim of the Tsaritsa gorge, with a tall
slate spire, arched windows, a bay and balconies with iron railings. Its roof is torn open on the
left, rafters bare, but the masonry stands. Below it the gorge slope is cut into terraced dugouts
roofed with planks and earth. Beyond: roofless one- and two-storey houses stepping down to the
Volga, pale sand shoals on the far bank. No snow.
Reproduce: a steep (30-40 degree) loess slope inside the city, with dugout roofs (about 3 x 2 m,
raised 0.3-0.6 m above the slope) terraced into it; a house that keeps its walls while its roof skin
and rafters fail; storeys about 3.5-4.0 m, windows about 2 m tall with arched heads, eaves about
10-11 m, spire top about 20 m (+/-20 %, no human in frame; scaled from the window and door
proportions).
Use: G (roof failure without wall failure), L (balka terrain inside the city), S (dugouts under
houses), M.

**[04-27]** `04_stalingrad_city/04_stalingrad-south-ruins-colour-autumn-1942_barch-169-0902.jpg`
Bundesarchiv Bild 169-0902, photographer not named, "1942 Herbst" (print strip; Commons date
1942-09-23 is a batch date), Stalingrad-South. Grade A-; [colour] (strong cyan shift).
Shows: a wide, rutted, unpaved street or square of pale dust. On the right four tall brick stacks
stand alone in front of a two-storey plastered building: what remains of a burned building. On the
left, beyond low houses, rows of chimney stubs mark a burned timber quarter. The Volga closes the
view.
Reproduce: burned timber quarters leave brick stoves and chimneys standing in rows at plot pitch
(about 10-20 m), with nothing between them but ash and low debris; free-standing stacks about
1.0-1.5 m square and roughly 9-14 m tall (+/-35 %: they appear twice the height of the two-storey
building behind them, at about 8 m, but stand nearer the camera by an unknown amount); street or
square width about 30-40 m (+/-30 %, from the single walker at about 40 m). Relative value: the
ground is lighter than the brick and lighter than the plastered walls in shade.
Use: G (burned wooden_house = stove and chimney only), L (southern fringe, wide unpaved streets), V
(relative value), M.

**[04-28]** `04_stalingrad_city/04_barmaley-fountain-station-square-1943_1943.jpg`
Sergey Strunnikov, 1943 (Commons, cropped), station square. Grade B (identity certain; year only;
thin snow suggests February-March 1943).
Shows: the "Children's Round Dance" (Barmaley) fountain, six child figures about 1.2-1.3 m tall on a
low circular drum, with the gutted station building behind: a long two-storey classical facade with
tall windows, pilasters and a parapet, open to the sky through every upper window.
Reproduce: the basin's outer wall is about 0.5-0.9 m high (+/-30 %: the camera is close to the rim
and the figures stand several metres further back, so the rim looks taller than it is) and its
render has spalled off in patches of 0.2-1.0 m, exposing brick; about 10 cm of snow on the rim; the
station facade stands to full height (about 12-14 m, +/-15 %) as a shell with no floors or roof
behind it. The ground is trampled snow with dark scraps of cloth.
Use: L and V (the landmark set; Act 1 fountain at (300, -150)), G (station shell).

### 2.5 Folder 05: Steppe and pocket (17)

Most of this folder is Sergey Strunnikov's series from the pocket after the surrender, dated only
"1943" on Commons; the manifest places it in the "Stalingrad area" from the series title. His dated
frames in the set run from January to March 1943 ([05-09] Pitomnik, January; [04-06] February;
[07-15] March), and the snow in these frames fits January-February, but the day and the exact place
of each frame are not recorded (grade C unless stated).

**[05-01]** `05_steppe_and_pocket/05_abandoned-vehicle-column-snow-road_1943.jpg`
Sergey Strunnikov, 1943, "Stalingrad. Abandoned vehicles" (Commons). Grade C (year only; see the
folder note).
Shows: a line of abandoned staff cars and a truck parked in echelon along one side of a snow road on
a valley floor, some burned (paint gone, dark scorched bodies), doors open; on the other side a
wooden farm cart with 1.1 m spoked wheels, a steel drum, and greatcoats, blankets, planks, boxes and
a rifle scattered on the snow. Snow-covered slopes with bare dark patches rise behind.
Reproduce: the road is a shallow packed layer (5-10 cm) with two ruts and a dark wet patch; the
debris lies on top of the snow, so it was dropped after the last snowfall; cars about 4.3 m long and
1.6 m tall; the column fills one side of a 6-8 m road and leaves the other passable.
Use: S (retreat column debris), M (pocket roads in balkas), L (wreck dressing).

**[05-02]** `05_steppe_and_pocket/05_typewriter-in-snow-abandoned-truck_1943.jpg`
Sergey Strunnikov, 1943, "Everything abandoned by the German invaders" (Commons). Grade C; [staged?]
(the typewriter is placed as a foreground motif); [print] (hard contrast, black border).
Shows: an office typewriter in the snow; behind it, on a low rise, a burned cargo truck with slat
sides and canvas bows; dry grass stems through a thin crust.
Reproduce: snow thin enough for grass stems to show (15-30 cm); isolated wrecks on the open steppe
with no road visible; small office objects far from any building.
Use: S (an office abandoned in the open: the typewriter is attested), M.

**[05-03]** `05_steppe_and_pocket/05_field-of-saddles-romanian-cavalry_1943.jpg`
Sergey Strunnikov, 1943, "All that remains of the Romanian cavalry" (Commons). Grade C.
Shows: hundreds of military saddles piled in rows in the foreground; stacks of crates and
cylindrical items beyond; a train of empty flat and low-sided wagons on a line along the horizon;
one soldier in a greatcoat for scale; thin, patchy snow.
Reproduce: supply dumps at railheads: rows and stacks 1.5-2 m high; flat wagons about 10-12 m long;
the horses are absent (the absence is the story); the soldier gives a 1.7 m scale at about 40 m.
Use: S (absence), M (railhead dump on the pocket's edge).

**[05-04]** `05_steppe_and_pocket/05_captured-sdkfz250-halftracks-snow_1943.jpg`
Sergey Strunnikov, 1943, captured German light armoured half-tracks (Commons). Grade C.
Shows: rows of armoured half-tracks parked side by side with lanes of about 2 m between them; visor
flaps raised, stowage bins full of snow, a Balkenkreuz and number plates legible; drifts up to 0.5 m
against the hulls; a trampled path of dark mud down the lane.
Reproduce: wind-packed snow collects on horizontal surfaces and in the lee of hulls, while lanes
used by feet go to mud; vehicle parks are dense (1-2 m gaps).
Use: M (pocket vehicle parks), V (snow on vehicles), S.

**[05-05]** `05_steppe_and_pocket/05_silenced-german-howitzer-battery_1943.jpg`
Sergey Strunnikov, 1943, "The German guns fell silent" (Commons). Grade C.
Shows: a long-barrelled German field gun, elevated, its recoil cylinders wrapped in sacking and
cord, standing in a shallow pit behind a heaped snow-and-earth parapet; two more guns and a loose
wheel about 60-150 m away on a featureless plain; planks and boxes scattered.
Reproduce: battery spacing of tens of metres; parapets 0.6-0.8 m high in front of the wheels (gun
wheel about 1.4 m); winter wrapping on mechanisms; nobody there.
Use: M (abandoned battery on the open ground west of the city), S.

**[05-06]** `05_steppe_and_pocket/05_abandoned-nebelwerfers-balka_1943.jpg`
Sergey Strunnikov, 1943, abandoned six-barrelled Nebelwerfer launchers (Commons). Grade C.
Shows: two 15 cm launchers on a valley floor, one close with its tubes toward the camera, one on its
split trail behind; a burned truck body at each edge of the frame; beyond, a long balka side rising
roughly 20-40 m (+/-50 %: no reference object stands at that distance), streaked where wind has
scoured the snow off grass and earth; a small vehicle on a track far off.
Reproduce: snow lies in hollows and on lee slopes and is scoured from convex ground, leaving dark
streaks that follow the contours; valley-floor tracks.
Use: M (balka terrain, wind-scoured slopes), V (snow distribution on relief).

**[05-07]** `05_steppe_and_pocket/05_air-supply-drop-containers_1943.jpg`
Sergey Strunnikov, 1943, German air-drop supply containers (Commons). Grade C.
Shows: rows of long cylindrical containers with rounded noses lying in the snow, each carrying
stencilled handling text, while two Soviet soldiers stand one on end: it is much taller than they
are, about 2.6-3.2 m long and 0.6-0.7 m in diameter (+/-20 %: it stands nearer the camera than the
men, so its apparent size is an upper bound).
Reproduce: air-supply debris laid in rows (collected), snow caps on each; stencils are part of the
object but must stay illegible at game resolution (no readable text).
Use: S (the air bridge), M (pocket dropping zones and airfields).

**[05-08]** `05_steppe_and_pocket/05_captured-airfield-ju87-he111-ju52_1943.jpg`
Sergey Strunnikov, 1943, captured German airfield near Stalingrad, name not given (Commons). Grade
C.
Shows: a Ju 87, a Ju 52 and several He 111 bombers parked almost wingtip to wingtip on trampled
snow; Soviet soldiers moving crates; round steel tubs and dark earth heaps in the foreground.
Reproduce: aircraft parked close with no dispersal; churned snow tracks between them; dark earth
spoil from dug positions.
Use: M (Pitomnik or Gumrak airfield set).

**[05-09]** `05_steppe_and_pocket/05_fw200-captured-pitomnik-airfield_1943-01.jpg`
Sergey Strunnikov, January 1943, Pitomnik airfield (Commons). Grade B.
Shows: a four-engined Fw 200 from low down, men under the wing, a ladder, trucks parked behind, a
flat snowy field to a low treeline.
Reproduce: the airfield is a flat, open snow plain; aircraft and vehicles are the only verticals.
Use: M (Pitomnik).

**[05-10]** `05_steppe_and_pocket/05_shot-down-ju52-fuselage-snow_1943.jpg`
Sergey Strunnikov, 1943, a Soviet officer at a shot-down Ju 52 (Commons). Grade C.
Shows: a fuselage (smooth-looking dark skin with a cross; corrugated panels at the break) torn open
at mid-length, frames, stringers and wiring exposed; snow drifted against the skin; the officer
(about 1.75 m) is shorter than the fuselage (about 2.5 m) at the break.
Reproduce: aluminium tears in jagged flaps; snow banks on the windward side.
Use: M (air-bridge wreck).

**[05-11]** `05_steppe_and_pocket/05_abandoned-21cm-mortar-barrel-wagon_1943.jpg`
Sergey Strunnikov, 1943, a 21 cm Mörser 18 barrel on its transport wagon near Stalingrad (Commons).
Grade C.
Shows: a soldier in a rain cape over his greatcoat and an ushanka with the flaps tied, leaning into
the wind beside a massive barrel frosted with rime; shrub stems stand 0.3-0.6 m out of the snow.
Reproduce: rime on cold metal on the windward side; snow 20-30 cm deep (boots sink to the calf);
wind posture.
Use: M (steppe wind and frost), people.

**[05-12]** `05_steppe_and_pocket/05_german-war-graves-gorodishche_1943.jpg`
Sergey Strunnikov, 1943, German soldiers' cemetery at Gorodishche (Commons). Grade C (place from the
description).
Shows: rows of fresh grave mounds, each about 2.0 x 0.8 m and 0.1-0.2 m high, marked with a German
steel helmet; some are longer raised rectangles with two or three helmets side by side; a few white
wooden markers; a long gable-roofed barn and burned vehicles behind; bare wet earth, thin snow only
on the hills beyond.
Reproduce: graves in straight rows with a narrow path; helmets as markers; the ground thawed and
dark. Restraint: no bodies, no text.
Use: S (graves as the aftermath), M.

**[05-13]** `05_steppe_and_pocket/05_car-blown-off-road-buried-in-snow_1943.jpg`
Sergey Strunnikov, 1943, "In the Stalingrad snows", an Opel Kadett thrown off the road (Commons).
Grade C.
Shows: a small saloon with its door open, sunk in a drift up to the windows; a slab of wind-packed
snow about 0.25 m thick on the roof; a wind scoop (a hollow) in the drift behind it; an embankment
beyond.
Reproduce: drifts 1.0-1.5 m deep against roadside obstacles and in cuttings, with scoured hollows on
the windward side and cornices on the lee.
Use: M (drift morphology), V.

**[05-14]** `05_steppe_and_pocket/05_steppe-bluff-colour-storch-overhead_barch-169-0005.jpg`
Bundesarchiv Bild 169-0005, photographer not named, August 1942, near Stalingrad. Grade A; [colour].
Shows: a soldier on a narrow ridge at the foot of a vertical, eroded loess face with grass on its
lip; a Fieseler Storch overhead.
Reproduce: balka sides break into near-vertical loess faces; this one rises about 4-5 m above the
soldier (+/-30 %, from the soldier at about its depth); grass tufts overhang the edge.
Use: M (balka edges).

**[05-15]** `05_steppe_and_pocket/05_steppe-fire-rolling-country-colour_barch-169-0938.jpg`
Bundesarchiv Bild 169-0938, photographer not named, September 1942 (archive caption; batch date
1942-09-23). Grade A; [colour] (cyan shift).
Shows: German soldiers in a captured jeep watching a steppe fire; long, gentle slopes and a shallow
valley; a black burned strip and a drifting smoke line; a horizon several kilometres off.
Reproduce: relief of 10-30 m over 1-2 km; sightlines of kilometres; burned ground in bands.
Use: M (open ground west of the city and the route west).

**[05-16]** `05_steppe_and_pocket/05_uranus-link-up-sovetsky-nov-1942_1942-11.jpg`
Unknown Soviet photographer, November 1942, Sovetsky (Commons; scan from an Italian book). Grade C;
[staged?] (commemorative moment, book-scan provenance, original caption not seen).
Shows: officers and soldiers in sheepskin coats, greatcoats and ushankas embracing on thin snow.
Reproduce: Soviet winter dress only.
Use: people.

**[05-17]** `05_steppe_and_pocket/05_relief-attempt-panzers-snow-road-dec-1942_1942-12.jpg`
Unknown photographer, December 1942, Operation Wintergewitter (Commons from a Russian web page).
Grade C.
Shows: a Panzer IV with the long 7.5 cm gun and a second tank and half-track on a snow road; a man
in a reversible winter anorak and dark trousers directs traffic.
Reproduce: 5-15 cm of snow, cut to earth in the wheel and track ruts; flat country, horizon in haze;
low winter sun from the side.
Use: M (the relief-attempt corridor and route west).

### 2.6 Folder 06: Winter conditions (14)

**[06-01]** `06_winter_conditions/06_infantry-riding-whitewashed-tiger-snowy-street_barch-101i-277-0846-13.jpg`
Bundesarchiv Bild 101I-277-0846-13, Jacob, January 1944, Ukraine. Grade A (but a year after
Stalingrad).
Shows: two whitewashed Tigers on a village street; about ten men in white hooded suits ride the lead
tank; the tracks are packed white with snow; a telegraph pole braced by a raking strut; bare trees
and low houses.
Reproduce: snow packs into track links and running gear; poles with raking stays; the village street
is an open snow space with no visible edge, wide enough for two Tigers abreast with room to spare
(at least 10 m).
Use: V (Tiger in snow), L (poles), M.

**[06-02]** `06_winter_conditions/06_tiger-ammunition-resupply-open-snowfield_barch-101i-278-0889-14.jpg`
Bundesarchiv Bild 101I-278-0889-14, Wehmeyer, January-February 1944. Grade A (1944).
Shows: two whitewashed Tigers on an open snowfield, crew standing on the deck passing rounds; a long
low barn with a snow-covered roof about 40 m away; the horizon is lost where snow meets overcast
sky.
Reproduce: under overcast the ground and sky have nearly the same value, and vehicles and buildings
are the only dark masses; no horizon line.
Use: V (overcast snowfield value range), M.

**[06-03]** `06_winter_conditions/06_halftrack-on-mud-track-snowy-forest_barch-101i-031-2406-17.jpg`
Bundesarchiv Bild 101I-031-2406-17, Schelm, date field 1942-12-22. Grade B (date field may be a
batch date).
Shows: an Sd.Kfz. 10 from behind on a black mud track through a deciduous wood lightly dusted with
snow; ruts full of mud; a fallen log.
Reproduce: traffic turns snow to mud; used tracks are dark even when everything else is white.
Use: M (tracks through woods), V.

**[06-04]** `06_winter_conditions/06_whitewashed-panzer-iv-stuck-shovelled-free_barch-101i-215-0354-14.jpg`
Bundesarchiv Bild 101I-215-0354-14, Gebauer, December 1941. Grade A (winter 1941-42).
Shows: a Panzer IV with striped and blotched white paint bellied in deep snow on a road, men
shovelling around the tracks; a snow bank about 1 m high along the road edge; a second tank behind;
a conifer edge.
Reproduce: deep powder buries running gear to the top of the road wheels (0.6-0.7 m); the road edge
becomes a bank of 0.9-1.1 m (hip height of the men).
Use: M (deep drifts and banks), V.

**[06-05]** `06_winter_conditions/06_radio-halftrack-in-snow-village-jan-1943_barch-101i-235-0962-05a.jpg`
Bundesarchiv Bild 101I-235-0962-05A, Mentz, January-February 1943 (print strip), southern front.
Grade A.
Shows: a small armoured half-track with a tarpaulin over its compartment and a rod aerial, men in
greatcoats and hooded white smocks walking beside it; a village of low hipped-roof houses on a flat,
open snow plain.
Reproduce: steppe villages: one-storey houses, hipped roofs, big open spaces between them; trampled,
compacted street snow.
Use: M (villages on the route west), people.

**[06-06]** `06_winter_conditions/06_infantry-column-winter-clothing-march_barch-101i-277-0847-14.jpg`
Bundesarchiv Bild 101I-277-0847-14, Jacob, January 1944. Grade A (1944).
Shows: German infantry in padded reversible suits, helmets and knitted caps, marching on a snow
road; thin telegraph poles with wires; a board fence; bare trees.
Reproduce: thin poles of roughly 6-8 m (+/-25 %; their spacing cannot be measured here); board and
picket fences of about 1.2-1.5 m along the plots.
Use: people (German winter dress), L and M (poles, fences).

**[06-07]** `06_winter_conditions/06_soldier-hooded-face-wrapped-cold_barch-101i-277-0850-09.jpg`
Bundesarchiv Bild 101I-277-0850-09, Jacob, January-February 1944. Grade A.
Shows: a soldier's face framed by a hood over a knitted toque, a scarf tied across the chin.
Reproduce: layered head coverings.
Use: people.

**[06-08]** `06_winter_conditions/06_soldiers-pulling-sledge-past-wrecked-valentine_barch-101i-277-0850-11.jpg`
Bundesarchiv Bild 101I-277-0850-11, Jacob, January 1944. Grade A (1944).
Shows: two men in white suits haul a boat-shaped sledge past a wrecked Valentine, a third walks
ahead: the track broken and lying in a heap, fenders bent, a second wreck about 20-40 m behind;
greatcoats, boxes and planks strewn on the snow; a wire fence and a hut on the horizon.
Reproduce: a knocked-out tank carries a debris halo 5-15 m across (dropped kit, planks, track
links); broken tracks lie in a folded heap beside the hull.
Use: S (the minutes after a tank is hit), L (wreck dressing), V.

**[06-09]** `06_winter_conditions/06_infantry-gun-loaded-in-snow-feb-1943_barch-101i-732-0128-03.jpg`
Bundesarchiv Bild 101I-732-0128-03, Schmidt, Georg, February-March 1943, Grossdeutschland division,
central sector (not the Stalingrad front). Grade A.
Shows: a German crew in greatcoats and hooded white smocks loading a light infantry gun; ammunition
boxes, spent cases and straw on trampled snow; a low brush line.
Reproduce: gun positions leave boxes, cases and straw bedding; 10-20 cm of trodden snow.
Use: S (occupied positions), L.

**[06-10]** `06_winter_conditions/06_winter-rifle-position-tent-shelter_barch-146-1992-055-33.jpg`
Bundesarchiv Bild 146-1992-055-33, Mährlen, 8 December 1941, near Koryakova, south-west of Moscow.
Grade A (winter 1941-42).
Shows: a hooded soldier in a scrape under a shelter-half pitched as a lean-to; a straw bundle as a
parapet; a Kar98k, a stick grenade and two egg grenades laid ready on the snow edge; a bundle of
cloth and a mitten beside them.
Reproduce: improvised positions: a shelter-half, straw, a scrape 0.4-0.6 m deep.
Use: S (a position recently manned), L.

**[06-11]** `06_winter_conditions/06_panje-sledge-in-blowing-snow_barch-183-b15084.jpg`
Bundesarchiv Bild 183-B15084, Lachmann, 15 January 1942, near Minsk. Grade A; [prop].
Shows: a horse under a wooden duga arch pulling a sledge, the driver on foot ahead of it, bent into
driving snow; the horse's legs lost in blowing ground snow; a tree line perhaps 100-200 m off is
only a faint grey smear.
Reproduce: ground blizzard: a layer of blowing snow about 0.3-0.5 m deep over the surface under a
whiter sky; contrast falls steeply with distance, so objects beyond a few tens of metres are
silhouettes and a tree line at 100-200 m barely separates from the sky.
Use: V and M (blizzard medium), S.

**[06-12]** `06_winter_conditions/06_soldiers-around-fire-short-lull_rian-137429.jpg`
RIA Novosti #137429, Georgiy Zelma, 7 January 1943, Stalingrad. Grade A.
Shows: some fifteen Soviet soldiers round a small fire under the broken eave of a ruin, seen from
inside; outside, a trodden snow field of 100-250 m with free-standing ruins: a tower-like wall
fragment of four storeys on the left, lower wall runs, and on the right a large block with a
stepped-gable skyline.
Reproduce: open ground between ruins in the centre by January 1943; free-standing wall fragments of
3-4 storeys; people shelter under whatever overhang survives; low sun, long shadows, bright snow
against dark interiors.
Use: L (open ground and ruin fragments), S (a fire in a ruin's lee), V (exposure: interior dark,
snow bright).

**[06-13]** `06_winter_conditions/06_improvised-straw-winter-boots_rian-604297.jpg`
RIA Novosti #604297, Georgiy Zelma, 1 February 1943, Stalingrad. Grade A.
Shows: a German prisoner in greatcoat and head scarf wearing woven straw overboots; a Soviet soldier
in sheepskin and valenki points at them; behind, a steep snowy slope with dark dugout openings, a
pole and stacked logs.
Reproduce: straw overboots about 0.3 m tall and twice the width of a boot; a steep slope, perhaps
15-20 m high (+/-50 %: it stands an unknown distance behind the men), cut with dugouts (a balka or
the Volga bank).
Use: people (prisoners, February 1943), S, L (dugout slope).

**[06-14]** `06_winter_conditions/06_captured-german-felt-guard-boots_1942-11.jpg`
Emmanuil Evzerikhin (TASS), November 1942, Stalingrad area (Commons). Grade B.
Shows: Soviet soldiers in quilted jackets, greatcoats and ushankas laughing as one tries on German
felt overboots with wooden soles; an ML-20 gun-howitzer behind them in a revetment of earth and snow
about 1.5-2 m high.
Reproduce: artillery revetments of dark earth with snow on top; the valenki and quilted jacket
silhouette.
Use: people, L (gun revetment).

### 2.7 Folder 07: People (17)

**[07-01]** `07_people/07_pow-column-through-stalingrad-street_1943-02.jpg`
Anton Zenin (TASS), February 1943, a Stalingrad street; prisoners taken south-west of the city.
Grade B.
Shows: an elevated view down a straight street with a column of prisoners some 400-500 m long and
four to six abreast, guards alongside. Foreground: a row of roofless one-storey brick houses seen
from above: thick perimeter walls (their tops read roughly 0.3-0.7 m, see G3) with window and door
holes, internal cross walls, snow filling the interiors, chimney stubs, fallen sheet roofing and a
twisted iron roof frame lying on the snow. Middle: a gutted two-storey brick house with a stepped
parapet and a gate arch; a burnt dark three-storey building with a gable; a surviving one-storey
house with a snow-covered hipped roof and smoke rising from its chimney; bare trees; poles with
crossarms. Behind: rows of gutted four- and five-storey 1930s blocks with flat roofs, regular window
grids, projecting stair bays and chimney stacks along the roofline, and a tall pale block in the
haze.
Reproduce: the district mixes typologies within one view: one-storey brick houses on the street
line, two-storey merchant houses with gate arches, and 1930s blocks set back in parallel rows. The
street is about 18-22 m between building lines (+/-25 %, from the column's width of about 3-4 m
against the street). Roofless one-storey houses keep all their walls to eaves height (about 3-4 m)
with snow 0.3-0.5 m deep inside. A few buildings still have roofs and are inhabited (smoke from a
chimney). Haze swallows buildings beyond about 500 m.
Use: L (primary: street width, mix and setback of typologies), G (one-storey brick house burned,
1930s block gutted), V (district street view and haze), S (the prisoner column).

**[07-02]** `07_people/07_pow-column-march-to-camp-feb-1943_barch-183-e0406-0022-010.jpg`
Bundesarchiv Bild 183-E0406-0022-010, photographer not named, February 1943 (ADN caption); the
column of prisoners from Stalingrad on the way to a camp. Grade A-; [prop] (GDR-era caption).
Shows: a long column of German prisoners in greatcoats, with blankets over their heads and
shoulders, walking up a snow road cut between banks about 1-1.5 m high; a hut, a truck and a field
kitchen on the rise; open snow plain; low sun with long shadows.
Reproduce: roads through snow are cut troughs with banks of 1-1.5 m thrown up by shovelling and
traffic; the prisoners' silhouette is a blanket over the head, a greatcoat and a bowed walk.
Use: S (the march out), M (roads in the open steppe), people.

**[07-03]** `07_people/07_pows-marched-past-grain-elevator_1943.jpg`
Photographer unknown (Soviet), 1943, the grain elevator (Commons; book scan). Grade C.
Shows: prisoners and Soviet guards walking past the elevator: the concrete working house with its
upper windows blown out and a pitted face, the long silo block with a shell-holed upper gallery, low
sheds; thin snow.
Reproduce: against the people at about 30-50 m, the silo block is about 30 m tall and the working
house about 50 m (+/-30 %: the people stand well in front). Shell hits on concrete are round, dark
holes 0.5-2 m with no collapse.
Use: L and V (the elevator landmark), G (concrete damage), S.

**[07-04]** `07_people/07_german-soldiers-taken-prisoner_1943-02.jpg`
Max Alpert (TASS), February 1943, near Stalingrad. Grade B.
Shows: a group of German prisoners at close range: greatcoats, field caps with the flaps down,
knitted toques, scarves and balaclavas, hoods improvised from blankets, stubble; behind them a
village of low houses and a collapsed lean-to shed of poles on a snow plain.
Reproduce: German prisoner dress in February 1943: layered, improvised, no white clothing; the
village has low timber and clay houses with snow on the roofs, and sheds of poles.
Use: people (primary German prisoner reference), M (villages round the pocket).

**[07-05]** `07_people/07_soviet-soldier-gives-light-to-pows_1943.jpg`
Pavel Troshkin, 1943 (probably February; Commons). Grade C (year only).
Shows: a Soviet soldier lights a cigarette for German prisoners; hoarfrost and fine snow on their
hoods, scarves, cloth and eyebrows; a blank white background of falling snow or mist.
Reproduce: rime on wool and fur collects on the windward and upper surfaces; faces and cloth read
dark against a white background.
Use: people, V (frost on cloth).

**[07-06]** `07_people/07_paulus-schmidt-adam-captured-64th-army_barch-183-f0316-0204-005.jpg`
Bundesarchiv Bild 183-F0316-0204-005, probably Georgy Lipskerov, 31 January 1943, at the
headquarters of the Soviet 64th Army (Beketovka). Grade A; [prop] (ADN/TASS caption).
Shows: Field Marshal Paulus in a greatcoat and peaked cap, with Schmidt and Adam behind, walking
from a staff car along a snow-covered village street; a log house with a carved eave board and a tin
roof; thin snow with dark footprints and patches of earth.
Reproduce: log houses with carved eave boards and sheet roofs in the southern suburbs; street snow
trodden to earth.
Use: S (the end of the battle, as a place and date only; the game must not stage it), M
(Beketovka-type village street), people.

**[07-07]** `07_people/07_snipers-white-suits-entering-ruin_rian-450.jpg`
RIA Novosti #450, Georgiy Zelma, 1 December 1942, Stalingrad. Grade A; [staged?] (a composed action
frame).
Shows: three snipers in hooded white suits and felt boots stoop through the ruins of a brick
building: a wall with a toothed, stepped breach, a round-arched doorway, bare brick heavily pitted
by fragments, pale vertical strips where plaster or snow survives in the angles, loose bricks and
lumps on thin snow; a sheet of roofing iron on the ground.
Reproduce: measured from the brickwork (course pitch about 31 px and stretcher about 98 px, so about
2.5 mm per pixel at the wall, which also puts the crouching man at about 1.45 m): the arched doorway
is about 1.25 m wide and 2.1 m to the crown (+/-15 %). Breach edges step back course by course and
brick by brick. Fragment pits of 3-10 cm, several per square metre, over the whole wall. Thin snow
(2-5 cm) with brick lumps showing through.
Use: G (brick bond, breach edges, arched openings), V (pitting density at close range), S, people
(white suits in use).

**[07-08]** `07_people/07_red-army-men-firing-from-rooftop-jan-1943_barch-183-e0406-0022-001.jpg`
Bundesarchiv Bild 183-E0406-0022-001, photographer not named, January 1943 (ADN caption). Grade A-;
[staged?].
Shows: about ten soldiers in white suits kneel behind a brick parapet on a snow-covered flat roof,
firing rifles and a submachine gun over it; below, the city: blocks of four to six storeys and roofs
in smoke and haze.
Reproduce: a flat roof with a brick parapet about 0.8-1.0 m high (+/-0.1 m, against the kneeling
men) and about 0.4-0.5 m thick; roofs carry 0.2-0.4 m of drifted snow; from a roof the view extends
over hundreds of metres of broken roofline.
Use: G (flat roofs and parapets of the 1930s blocks), S (firing positions above street level), V.

**[07-09]** `07_people/07_smg-gunners-in-camouflage-suits-dec-1942_1942-12.jpg`
D. A. Minsker, December 1942, Voronezh Front (Glavarkhiv Moskvy, CC BY 4.0). Grade B (not
Stalingrad).
Shows: submachine gunners in hooded white suits with PPSh crouching and advancing beside a wrecked
brick building with a snow-covered slab of roof; bare trees; weeds through the snow.
Reproduce: the white suit's hood, loose smock and trousers; men kneeling in 20-30 cm of snow.
Use: people.

**[07-10]** `07_people/07_soviet-soldiers-search-factory-shop-snow_1943-01.jpg`
Anatoly Arkhipov, January 1943, a destroyed shop of a Stalingrad factory. Grade B; [staged?].
Shows: two soldiers in white suits with submachine guns step through deep, smooth snow over buried
rubble in a roofless shop: a brick pier broken off with its brickwork stepped and toothed, bent
steel rods and pipes across the frame, cables, a long low building beyond; hard, low winter sun with
long shadows and bright snow.
Reproduce: inside roofless buildings the snow drifts smooth over rubble into rounded mounds
0.3-0.8 m deep, with the tops of bricks and steel poking through; the men sink to the calf. Under
clear winter sun the snow's lit faces are the brightest values in the scene and its shadow faces
stay mid-grey, well above the dark cloth and never black. (Their blue cast under a clear sky is not
visible in a black-and-white print; it must come from the renderer's sky light.)
Use: L (snow over rubble in yards and interiors), V (clear-sky winter sun on snow), M.

**[07-11]** `07_people/07_13th-guards-soldiers-resting-cellar_1942-12.jpg`
Photographer unknown, December 1942: soldiers of 13th Guards Rifle Division at rest (Commons,
PD-Russia-2008). Grade C (anonymous; unit named); [staged?] (posed, flash-lit).
Shows: a cellar room: a samovar with a jug on it, a man lying on sacks and a quilted bedroll, a
PPSh, an iron bed spring on the floor, a blanket or tarpaulin hung on the wall, a fur cap on a nail,
men in quilted jackets and fur caps; rough plastered walls.
Reproduce: cellars are furnished with things carried from the flats above: samovar, bedsteads,
blankets hung as curtains and draught screens. Light is the flash of the print; physically the
cellar's light is a lamp or a stove.
Use: S (a lived-in cellar shelter), G (cellar room use and dressing).

**[07-12]** `07_people/07_german-mg34-team-firing-from-window_barch-101i-617-2571-04.jpg`
Bundesarchiv Bild 101I-617-2571-04, Ollig, Stalingrad; date field 1942-09-23 (a batch date; the
summer uniforms and the absence of snow point to September-October 1942). Grade A-.
Shows: three German soldiers in a dark room firing an MG 34 on its bipod from a window sill and a
rifle from the upper part of the window; cracked plaster walls with patches fallen; a crate and a
bench; outside, the collapsed sheet-iron roof of a one-storey house with bare rafters and a brick
chimney, gutted buildings beyond.
Reproduce: firing from windows over the sill; the sill about 0.9 m above the floor (the kneeling
gunner's elbows at the sill); windows tall enough (about 2 m) for a second man to fire standing
above the gun; interior plaster cracks in a network and falls in patches; the view from an upper
floor looks down onto one-storey roofs that have collapsed in the fire.
Use: S (a German fighting position inside a flat), G (window proportions, interior finish), V (dark
interior against a bright window).

**[07-13]** `07_people/07_german-soldiers-in-ruins-winter-42-43_1942.jpg`
Photographer unknown, captioned "winter 1942-43, Stalingrad" (Commons; scan from a book). Grade C
(date and place unverified).
Shows: German soldiers in helmets (one whitewashed) and padded winter suits at a cellar entrance in
a ruined building: a segmental brick arch about 1.2 m wide at rubble level under a heavy squared
timber beam, a plank laid across the rubble, stepped brick walls, plaster remnants, a broken
staircase or sloping boards.
Reproduce: cellar entrances survive as brick arches at the foot of the rubble, often the only way
into the cellar; heavy timbers and planks bridge the rubble.
Use: G (cellar entrance under rubble), S (the cellar as a position), people (German winter dress).

**[07-14]** `07_people/07_residents-returning-sledge-feb-1943_1943-02.jpg`
Yefim Kopyt, February 1943, Stalingrad. Grade B.
Shows: two women in headscarves, dark coats and felt boots cross a wide snowy street, one pulling a
small wooden sledge of bundles; right: four tall free-standing brick stacks in two pairs, each with
a wider lower stage and a narrower upper shaft under a projecting cap, render peeling; middle: a
roofless merchant house with moulded window surrounds and a dentil cornice, soldiers standing at its
foot; the arcaded shell of another building; a truck with its bonnet open on the left; bare trees,
poles and fallen wires; debris lying on the snow; low sun.
Reproduce: measured against the standing soldiers (about 77 px for 1.7-1.75 m, so about 44 px per
metre at their depth), with the bases of the left-hand stacks at the same image height as the
soldiers' feet on flat ground, so at a similar depth: the stacks stand free at about 12-13 m
(+/-20 %) and are about 1.3-1.5 m wide at the lower stage. The merchant house beside the soldiers
rises only about 6-7 m above the snow to the top of its cornice: a low ground floor or semi-basement
under one main floor, part of it perhaps buried by snow and rubble. The stacks are twice its height,
so they served a larger building of two or three storeys. Merchant houses keep their ornamented
brick facades as shells. Snow is 5-10 cm, trodden, with dark debris lying on top. Low sun makes long
shadows.
Use: G (merchant_brick_house facade and burned-building stacks), L (street and square with wrecks),
S (the return of civilians, February 1943), V.

**[07-15]** `07_people/07_teacher-with-children-march-1943_1943-03.jpg`
Sergey Strunnikov, March 1943, a street in Stalingrad. Grade B.
Shows: a teacher and about twenty children in padded coats, fur hats, headscarves and felt boots
lined up in front of a large four-storey rendered 1930s school or institute: windows in groups of
two and three, some frames and glazing bars still in place, several openings blown into ragged
holes, a breach through the top two storeys where the roof and part of the top floor have collapsed;
a stair tower at the right end; a row of bare shrubs along the front; the ground is thawing, wet and
dark, with slush and scattered debris.
Reproduce: the "shelled" condition as distinct from "burned": the facade stands with its window grid
intact, most frames survive, and damage is local (holes blown through the masonry and one collapse
through the top storeys) rather than total. Measured on the near-frontal right wing: windows about
0.24 of the storey pitch wide and 0.55 of it tall, grouped in threes on axes about 0.42 of the pitch
apart, with wide piers between groups; at a storey pitch of 3.8-4.2 m for a school (+/-15 %,
assumed, not measured) the windows are about 0.9-1.0 m wide and 2.1-2.3 m tall. Two round shell
holes of about 0.5 m sit side by side in one pier. Thaw in March turns the ground dark and wet.
Use: G (apartment_block or civic block, shelled), S (civilians return), V (thaw light and ground).

**[07-16]** `07_people/07_two-women-cross-fallen-fighters-square_1942-10.jpg`
Attributed to Emmanuil Evzerikhin, dated October 1942, the Square of Fallen Fighters (Commons;
Nationaal Archief print of an Anefo copy). Grade C for the date: the women's summer dresses and bare
legs point to late August or September 1942, soon after the bombing of 23 August.
Shows: two women carrying a basket, a bundle and a case across the square; a column of smoke; left:
an ornate five-storey building with iron balconies, still roofed; centre: a 1930s building whose
side wall has fallen, leaving its concrete floor slabs projecting at every storey and a tall
slab-and-pier fragment; a neighbouring party wall exposed with the ghost lines of floors and rooms;
right: a gutted 1930s block with a glazed stair strip, a four-storey merchant building with a
crenellated parapet and arched windows whose upper openings show sky, and a long two- to
three-storey classical building; globe lamp posts; rubble heaps along the building line.
Reproduce: the two floor systems fail differently: reinforced-concrete floors stay as slabs spanning
between surviving walls and piers even when the walls fall, while timber-floored brick buildings
become hollow shells. A party wall exposed by a neighbour's collapse carries the outline of each
floor and room. The square is an open paved or earth plain of 100 m and more.
Use: G (primary: RC slabs versus timber floors; exposed party walls), L and V (the central square in
autumn 1942), S (civilians under the bombing).

**[07-17]** `07_people/07_soldiers-and-villager-colour-sept-1942_barch-169-0023.jpg`
Bundesarchiv Bild 169-0023, photographer not named, September 1942, a village near Stalingrad. Grade
A; [colour].
Shows: two German soldiers eating from mess tins and a village woman sitting in a yard; a woven
wattle fence about 1.5 m high with jugs and pots hung upside down on its stakes; a small whitewashed
clay-rendered outbuilding with a ragged straw or reed roof and a dark door opening; barrels used as
seats and tables; bare earth.
Reproduce: steppe villages use wattle fences and clay-rendered, whitewashed buildings under straw or
reed roofs; relative value: the whitewashed wall is lighter than the earth and much lighter than the
roof.
Use: M (villages on the steppe and in the pocket), G (a rural timber-and-clay variant, not the
city's wooden_house), people.

### 2.8 Folder 08: Damage and recovery (17)

**[08-01]** `08_damage_and_recovery/08_tiger-hit-on-side-armour-inspected_barch-101i-022-2935-24.jpg`
Bundesarchiv Bild 101I-022-2935-24, Wolff/Altvater, Operation Citadel, July 1943 (date field
1943-06-21 is a batch date). Grade A.
Shows: a crewman's fingers at a non-penetrating hit on the Tiger's side plate: a round crater about
10-12 cm across with a raised, torn lip, paint flaked around it, next to the white-outlined cross.
Reproduce: a hit that fails to penetrate still removes metal: crater about 1.2-1.5 calibres across
(+/-20 %, from the finger width) with a raised rim.
Use: V (hull hit marks as geometry and material state, not a decal).

**[08-02]** `08_damage_and_recovery/08_tiger-antitank-gun-hit-gouge_barch-183-j15027.jpg`
Bundesarchiv Bild 183-J15027, Bernd Lohse, 10 August 1943 (original caption). Grade A; [prop].
Shows: a crewman at a shallow, bright, scooped gouge about 15 x 10 cm in the turret-front plate,
with many small splash pits around it.
Reproduce: ricochet gouges are long scoops with bright metal, not round holes; small-arms and
fragment pits (about 1 cm) pepper the whole face.
Use: V.

**[08-03]** `08_damage_and_recovery/08_tiger-turret-battle-damage-on-trestles_barch-101i-022-2923-13a.jpg`
Bundesarchiv Bild 101I-022-2923-13A, Kipper, 1943 (Kursk-period batch). Grade A.
Shows: a removed Tiger turret on heavy timber trestles, its mantlet gouged, torn pieces of armour
set on the hull edge in front.
Reproduce: mantlet gouges several cm deep; broken plate edges are jagged, crystalline.
Use: V (Tiger damage states).

**[08-04]** `08_damage_and_recovery/08_tiger-114-towed-by-sdkfz9-halftrack_barch-101i-022-2926-11a.jpg`
(`manifest.json` of 19:34 lists this file as `08_tiger-114-towed-by-two-sdkfz9_…`; the set's builder
renamed it after that manifest was written.) Bundesarchiv Bild 101I-022-2926-11A, Altvater,
Operation Citadel, July 1943 (date field June 1943 is the PK batch). Grade A.
Shows: Tiger 114 towed along a dirt track at the foot of a chalk slope by two 18-tonne half-tracks
(Sd.Kfz. 9) in tandem, one with a camouflage net over its load; a motorcycle alongside; a man on the
Tiger's deck; grass in the foreground.
Reproduce: a disabled Tiger needs two 18-tonne tractors coupled in line; the convoy is about 25-30 m
long. The chalk slope is pale where eroded.
Use: M (recovery on the summer and autumn routes), S (recovery as the fate of a disabled Tiger).

**[08-05]** `08_damage_and_recovery/08_tiger-stuck-in-marshy-river-recovery_barch-183-j15570.jpg`
Bundesarchiv Bild 183-J15570, Wolff/Altvater, end of September 1943 (ADN caption). Grade A.
Shows: a Tiger (number 32x) bogged in the churned mud of a stream crossing, towed out by a second
Tiger (seen from its deck) on twin steel cables; maize, thatched white cottages and poplars and
spruces behind.
Reproduce: black churned mud to the top of the running gear; tow cables about 30 mm.
Use: M (river crossings in later acts), V.

**[08-06]** `08_damage_and_recovery/08_tiger-334-track-repair-village_barch-101i-022-2948-05.jpg`
Bundesarchiv Bild 101I-022-2948-05, Paul Wolff, Operation Citadel, July 1943. Grade A.
Shows: Tiger 334 beside a thatched barn, the crew (one stripped to the waist) working at the
sprocket with a length of track on the ground; men resting on the turret.
Reproduce: thatch 0.3-0.5 m thick, eaves about 2 m above ground (+/-0.2 m, from the crew).
Use: M (steppe villages, summer acts).

**[08-07]** `08_damage_and_recovery/08_tiger-track-damage-snowfall-village_barch-101i-458-0077-12.jpg`
Bundesarchiv Bild 101I-458-0077-12, Stöpfgeshoff, winter 1943-44 (print strip). Grade A (1943-44).
Shows: a whitewashed Tiger waiting on a village road in falling snow under large bare trees
(15-20 m, dense twig crowns); a Kübelwagen and a car behind; a house with a white roof.
Reproduce: falling snow visible as short streaks close up; background houses at 100-200 m fade to
grey; large trees line village roads.
Use: V (snowfall medium), M.

**[08-08]** `08_damage_and_recovery/08_tiger-winter-field-workshop-overview_barch-101i-278-0875-17a.jpg`
Bundesarchiv Bild 101I-278-0875-17A, Wehmeyer, January-February 1944. Grade A.
Shows: a field workshop yard in thaw: a steel gantry crane (about 8 m high and 10 m span, +/-20 %,
against the Tiger hull below it) lifting a turret, a turretless Tiger hull, a turret propped on
stacks of 200-litre drums, a brick shed, and deep black ruts full of meltwater.
Reproduce: thaw turns yards to deep mud; drums as universal props and supports.
Use: M (rear-area workshop set), S.

**[08-09]** `08_damage_and_recovery/08_tiger-turret-lift-winter-workshop_barch-101i-278-0875-28a.jpg`
Bundesarchiv Bild 101I-278-0875-28A, Wehmeyer, January-February 1944. Grade A.
Shows: a Tiger turret raised about 1 m clear of the hull on the gantry, fitters on the deck,
officers watching, drums and log stacks around; slush and mud.
Reproduce: the scale of the turret against men; mud and slush in a used yard.
Use: M, V (Tiger scale).

**[08-10]** `08_damage_and_recovery/08_knocked-out-tiger-turret-thrown-off_nara-111-sc-171654.jpg`
NARA 111-SC-171654, Haber, 23 April 1943, Béja, Tunisia. Grade A.
Shows: a destroyed Tiger: the hull with its empty turret ring (about 1.8 m across) on a bank, the
turret lying beside it on its side with its roof torn open and interior exposed, the barrel on the
ground with its muzzle brake toward the camera, an 88 mm round (about 0.9 m) stood upright for
scale; a second Tiger wreck lying tilted on the slope behind with US soldiers on it; a pole; a
gravel road.
Reproduce: a catastrophic loss separates turret from hull, and the gun lies at ground level; the
turret roof tears open; the wreck's weight crushes the road verge.
Use: V (the Tiger's own catastrophic end state), S.

**[08-11]** `08_damage_and_recovery/08_knocked-out-t34-still-smoking-pokrovka_barch-101i-219-0553a-36.jpg`
Bundesarchiv Bild 101I-219-0553A-36, Koch, Operation Citadel near Pokrovka, July 1943. Grade A.
Shows: a knocked-out T-34 with the early cast turret (rounded front, rail along the side), smoke
still rising from the turret and hull, one track off so that the road wheels stand bare in charred
ground, fenders crumpled; a German soldier looks in.
Reproduce: T-34 wreck states: track off, fenders deformed, smoke from the fighting compartment,
scorched ground round the hull.
Use: V, S.

**[08-12]** `08_damage_and_recovery/08_knocked-out-t34-stalingrad-street_barch-183-b22359.jpg`
Bundesarchiv Bild 183-B22359, Herber, 5 October 1942, Stalingrad. Grade A.
Shows: a knocked-out T-34 head-on, a few metres from the camera, in a broad street or square with a
double tram track curving through the dirt surface; on the left a long blank brick wall with
pilasters and small gabled caps (the street wall of a one-storey warehouse or a yard wall) and a
pole with crossarms; on the right, well beyond the tank, gutted blocks of four to five storeys, a
stepped corner tower and a bare tree; rubble heaps along their base; no snow (October).
Reproduce: tram rails lie flush with or slightly proud of the dirt surface, with the sleepers
buried; blank street walls with pilasters at a regular interval (roughly 3-4 m, +/-30 %) and about
3-4 m high (uncertain); gutted blocks keep their street walls to full height. The open space is
wide, at least 20 m, but the tank is too close to the camera to scale it (an earlier reading of
20-25 m from the T-34 is withdrawn).
Use: L (tram street, wreck as cover), G (blank pilastered street wall of a warehouse or yard), V.

**[08-13]** `08_damage_and_recovery/08_destroyed-soviet-tank-snow-ruins-jan-1943_nac-2-942.jpg`
NAC 2-942, Herber (German PK), January 1943, Stalingrad. Grade A. The caption says KV-1, but the
small offset turret and small road wheels point to a light tank (T-60 or T-70 class); the type is
uncertain.
Shows: a snow-capped wrecked tank in an open square about 15-20 m from the camera, a broken timber
pole leaning at about 70 degrees in front of it; on the right, much nearer, a merchant building's
corner with a rusticated ground floor, a large shop window, shell and bullet pocks, and a
cantilevered balcony whose iron railing is torn and bent outward; beyond the square, gutted three-
and four-storey classical blocks, a corner tower, lamp posts and a shed.
Reproduce: 20-30 cm of even snow on the square with footprints; plinth and ground floor rusticated
in bands; the shop window about half the ground floor's height and nearly square; the balcony slab
at the top of the ground floor. With a ground floor of 4.5-5 m (assumed; the tank stands too far
behind the corner to scale it) the window is about 2.2-2.5 m each way and the balcony at about 5 m
(+/-25 %). Railings twisted outward; pockmarks over the whole corner, densest at ground floor.
Use: G (merchant_brick_house facade and plinth), L (a central square in January), V.

**[08-14]** `08_damage_and_recovery/08_abandoned-panzer-and-trucks-in-gully_1943.jpg`
Sergey Strunnikov, 1943 (Commons). Grade C.
Shows: a Panzer III tipped at about 45 degrees into a trench or gully about 1.5-2 m deep, its cross
and patchy whitewash visible; a burned truck (cab stripped of paint) and a cargo truck on the rim;
clods of dark earth, paper, sheet metal and a bent frame spread over thin snow.
Reproduce: wrecks at gully edges; earth clods thrown onto snow; debris fields of paper and sheet
metal.
Use: M (balka roads), S, V.

**[08-15]** `08_damage_and_recovery/08_panzer-iii-repair-sdkfz9-crane-jan-1943_barch-101i-235-0996-02a.jpg`
Bundesarchiv Bild 101I-235-0996-02A, Falk (PK 695), January 1943, southern front ("11th Panzer
Division?" in the archive caption). Grade A; the nearest photograph in the set to the German side's
own January 1943 winter outside the pocket.
Shows: an 18-tonne half-track with a jib crane (Sd.Kfz. 9/1) lifting a component beside a
whitewashed Panzer III, whose white wash is worn to the base colour on the edges; crewmen on the
tractor and on the ground; a steel farm wind pump with a multi-bladed wheel on a lattice tower about
10-12 m tall; young bare trees; a village of low houses behind; thin trodden snow.
Reproduce: the whitewash wears first on edges, hatches and the running gear; winter field repair
happens in the open at the edge of a village; a wind pump is a characteristic village-edge vertical
on the southern steppe.
Use: M (villages on the route west in January 1943), V (worn whitewash), S (field repair).

**[08-16]** `08_damage_and_recovery/08_destroyed-t34-mius-closeup_barch-101i-240-2130-04.jpg`
Bundesarchiv Bild 101I-240-2130-04, Hodea (PK 695), August 1943, Mius (Donets basin). Grade A.
Shows: a knocked-out T-34 Model 1943 with the hexagonal turret among tall sunflower stalks; its
hatch open; a man at the turret; track still on; tow cable slack along the hull; external fuel tanks
on the hull side; no obvious fire damage.
Reproduce: not every knocked-out tank burns: an intact-looking hull with its hatches open is a
common end state; the hexagonal turret with its two roof hatches marks the 1943 production.
Use: V (T-34 wreck states, 1943 turret), M (later acts in summer).

**[08-17]** `08_damage_and_recovery/08_tank-wreck-blown-apart_barch-101i-220-0630-02a.jpg`
Bundesarchiv Bild 101I-220-0630-02A, photographer not named (PK 694), September 1943 (date field
1943-09-23 is a batch date). Grade A-.
Shows: the wreck of a tank torn apart by an internal explosion beside a dirt road: the hull split
along its length, side plates and the deck blown outward, the running gear and a track run lying in
place on the left; road wheels thrown clear; plates, springs and fragments scattered over 10-15 m of
road verge; a motorcyclist passes in dust; poplars and orchards behind.
Reproduce: an ammunition explosion splits a tank into plates and throws road wheels and parts
10-20 m; the debris lies on the surface in a fan away from the hull. A catastrophic destruction
state must come from the damage model, never a swapped prop.
Use: V (catastrophic end state), S, M.

### 2.9 Folder 09: Equipment details (13)

**[09-01]** `09_equipment_details/09_88mm-round-handed-up-to-tiger_barch-101i-022-2948-24.jpg`
Bundesarchiv Bild 101I-022-2948-24, Paul Wolff, July 1943 (Citadel; batch date 1943-06-21). Grade A.
Shows: a crewman in a black side cap and shirt lifts an 8.8 cm round overhead to a man standing on
the track guard of Tiger 123; open grassland; a vehicle hung with branches.
Reproduce: a round is about 0.9-1.0 m long (+/-0.1 m, against the men's forearms and heads) and is
handed up one at a time; the track guard is about 1.2 m above the ground.
Use: S (resupply), V (round and hull scale).

**[09-02]** `09_equipment_details/09_88mm-rounds-laid-on-tiger-front-belgorod_barch-183-j14931.jpg`
Bundesarchiv Bild 183-J14931, Rottensteiner, July 1943, near Belgorod (Waffen-SS crew). Grade A;
[prop] (ADN caption).
Shows: two crewmen in camouflage smock and caps on the hull front, one passing a round; six rounds
laid across the front plate before stowage; a helmet hung on the hull; the cupola lid raised; a
tarpaulin.
Reproduce: rounds laid out on the hull before stowage: pale brass cases and dark projectiles against
the steel.
Use: S (resupply), V.

**[09-03]** `09_equipment_details/09_tiger-loading-shells-winter-sidecar_barch-101i-277-0847-31.jpg`
Bundesarchiv Bild 101I-277-0847-31, Jacob, January-February 1944. Grade A.
Shows: a whitewashed Tiger loaded through its turret roof, a round passed up by hand; dark wooden
ammunition crates with pale labels stacked about 1.2 m high beside the hull; a sidecar motorcycle
with a helmet on it and a spare wheel; a wooden crate; telegraph poles and a barn on a flat
snowfield.
Reproduce: crates stacked beside the vehicle for a resupply; whitewash streaked over the rear
stowage bin and hull and worn off round the hatches.
Use: S (resupply), V (winter Tiger at a halt).

**[09-04]** `09_equipment_details/09_tiger-refuelling-from-drums-hand-pump_barch-146-1978-107-06.jpg`
Bundesarchiv Bild 146-1978-107-06, photographer not named, 1943. Grade B (year only).
Shows: a Tiger's rear quarter: a man at a 200-litre drum with a hand pump in its bung, a hose up to
a man on the engine deck, another passing up a pipe, ammunition boxes on the grass; a woven mat or
net laid along the hull side; the turret number.
Reproduce: refuelling by hand pump from drums lying on their side; hoses; the crew spread over deck
and ground.
Use: S (refuelling at a halt), V.

**[09-05]** `09_equipment_details/09_repair-soldier-resting-on-stacked-shells_barch-101i-022-2923-10a.jpg`
Bundesarchiv Bild 101I-022-2923-10A, Kipper (PK 637), 1943 (Kursk period; batch date). Grade A.
Shows: a repair-company soldier asleep on a heap of large-calibre projectiles stacked against a
brick wall, base plates outward; a leather bag, a mess tin and a paint tin; a vehicle with a cross
behind.
Reproduce: ammunition and projectiles stacked loose in heaps against walls; each projectile about
0.4-0.6 m long (+/-20 %, against the man).
Use: S (a dump, exhaustion), L (yard dressing).

**[09-06]** `09_equipment_details/09_dugout-stove-mess-tins_barch-101i-394-1459-29.jpg`
Bundesarchiv Bild 101I-394-1459-29, W. Wanderer, about April 1942. Grade A.
Shows: two Luftwaffe soldiers crouching by a cylindrical sheet-metal stove about 0.3 m across and
0.6-0.7 m tall, its pipe spliced from sections rising to the roof; mess tins, kindling and boards; a
rough earth wall; the scene lit from one side.
Reproduce: the stove is the dugout's heat and, with its door open, a light source; its pipe runs up
through the roof; walls of bare earth; boards on the floor. The stove must be a modelled emitter,
not fill light.
Use: S (dugout and cellar life), G (cellar dressing), V (the stove as the emitter).

**[09-07]** `09_equipment_details/09_log-bunker-entrance-cholm_barch-183-j20435.jpg`
Bundesarchiv Bild 183-J20435, Richard Muck, "1942 Anfang" (print strip), Kholm. Grade A; [prop]
(caption). No snow and a man in shirtsleeves suggest spring 1942 rather than January.
Shows: a soldier cleaning his boots at the entrance of a bunker dug into a slope: squared posts and
a lintel frame the door, the side wall is cribbed with horizontal logs, firewood is stacked inside
the entrance; a board path with a pole handrail runs along the slope; houses and bare trees beyond.
Reproduce: bunkers in slopes are log-cribbed (logs 0.2-0.3 m) with entrances about 0.9-1.0 m wide
and 1.9-2.0 m high; paths along slopes are boarded and railed.
Use: S (dugouts in slopes), L and M (the Volga bank and balka dugouts, compare [04-21], [04-22]).

**[09-08]** `09_equipment_details/09_signaller-radio-telephone-snow-trench_barch-101i-278-0895-27.jpg`
Bundesarchiv Bild 101I-278-0895-27, Wehmeyer, January-February 1944. Grade A.
Shows: a signaller in a padded winter suit and knitted cap in a snow trench with a telephone handset
at his ear; the telephone case on a plank at the trench edge; cables laid over the snow.
Reproduce: trenches cut about 1 m into snow and earth with planks laid on the parapet; telephone
cables lying on the snow surface.
Use: S (field telephone lines), people.

**[09-09]** `09_equipment_details/09_radio-operator-in-bare-room_barch-101i-031-2449-35a.jpg`
Bundesarchiv Bild 101I-031-2449-35A, Fenske, 1942. Grade A- (year only).
Shows: a radio operator in a fur-lined hood sitting on the floor under a blanket beside his set; a
large barrel; a bare room with stained, cracked plaster; two tall two-light windows with glazing
bars that are the only light.
Reproduce: in a house the light is the windows: bright sills, a fall-off across the room, dark
corners; windows about 0.9 m wide and 1.6-1.8 m tall with sills about 0.9 m up (+/-15 %, against the
seated man).
Use: G (simple house windows, interior finish), V (window-lit interior), S (a signals post in a
house).

**[09-10]** `09_equipment_details/09_armoured-vehicle-radio-station-interior_barch-101i-268-0158-13.jpg`
Bundesarchiv Bild 101I-268-0158-13, Utecht, October 1941 (Sd.Kfz. 253, not a Tiger). Grade A.
Shows: a radio operator in the cramped compartment: vision blocks glowing white at eye height, a
large dial, switches, small labels, stowage boxes; the roof close overhead.
Reproduce: inside armour, the vision blocks are the brightest things in view and the rest is lit
only by what bounces off light-painted walls (compare [01-16]).
Use: V (interior light in armour).

**[09-11]** `09_equipment_details/09_nco-with-mp40-winter_barch-101i-278-0899-26.jpg`
Bundesarchiv Bild 101I-278-0899-26, Wehmeyer, January 1944. Grade A.
Shows: a young NCO with a whitewashed helmet whose wash is chipped, a greatcoat and knitted gloves,
an MP 40 on a sling across his chest, sitting in snow.
Reproduce: whitewashed helmets with chipped wash; the MP 40 carried slung across the chest.
Use: people.

**[09-12]** `09_equipment_details/09_officers-map-board-colour-near-stalingrad_barch-169-0952.jpg`
Bundesarchiv Bild 169-0952, photographer not named, late summer 1942, near Stalingrad. Grade A;
[colour].
Shows: a first lieutenant and a lieutenant sitting in a scrape in knee-high dry steppe grass with a
map board; a lumpy bank of freshly dug earth in the foreground; low sun and long shadows; a hazy
ridge on the horizon.
Reproduce: the steppe near Stalingrad in late summer: dry grass about 0.5 m high; freshly dug earth
in lumps; relative value: dug earth is darker than the dry grass.
Use: M (the steppe in late summer), people.

**[09-13]** `09_equipment_details/09_erkennungsmarke-id-tag-handbook-page_1943.jpg`
US War Department, Handbook on German Army Identification, 1943, page 29 (Commons scan). Not a
photograph. Grade B (as a document).
Shows: the text on the identification tag and a drawing of one: an oval tag divided by perforations
into two halves, each stamped with the same personal number, unit and blood group. The page says the
tag is zinc and about 2 by 2¾ inches (51 x 70 mm), and that the lower half is broken off when a man
is killed while the upper half stays with the body. (The manifest's guidance says aluminium; this
source says zinc.)
Reproduce: the two-part oval tag on a neck cord; a tag broken along its perforation is an attested
trace of a death. Its stamped markings are text and must stay unreadable in game.
Use: S (a broken tag as a restrained trace).

### 2.10 Folder 10: Later acts, 1945 (6)

**[10-01]** `10_later_acts_1945/10_chuikov-observation-post-seelow_1945-04.jpg`
Photographer not named (published by the Russian Ministry of Defence), April 1945, the 8th Guards
Army observation post, Seelow Heights: V. I. Chuikov (centre) and A. M. Pronin (right). Grade B.
Shows: three officers in greatcoats and peaked caps behind a pale parapet with a scissors periscope
and binoculars; a camouflage net draped over pine branches overhead; earth walls and a plank frame.
Reproduce: observation posts are trenches with timber-framed parapets under nets and cut pine;
scissors periscopes on the parapet.
Use: M (the Seelow act), people.

**[10-02]** `10_later_acts_1945/10_soviet-artillery-line-before-berlin_barch-183-e0406-0022-012.jpg`
Bundesarchiv Bild 183-E0406-0022-012, photographer not named, April 1945, before Berlin. Grade A-;
[prop] (ADN caption).
Shows: a line of at least six ZiS-3 guns at 20-30 m intervals on flat farmland, crews in greatcoats,
ammunition boxes stacked by each gun, spoil heaps and a slit trench in the foreground, wheel ruts; a
village with tree lines on the horizon.
Reproduce: gun lines on the Oder plain at 20-30 m intervals, boxes stacked three or four high, slit
trenches beside the guns; a flat horizon with village tree lines at 2-3 km.
Use: M (the Oder and Seelow act).

**[10-03]** `10_later_acts_1945/10_soviet-officer-and-soldier-before-reichstag_barch-204-009.jpg`
Bundesarchiv Bild 204-009, Henryk Gorovits, May 1945, Berlin. Grade A.
Shows: a Soviet officer and soldier on the Reichstag forecourt: the blackened, pitted sandstone
corner tower and side facade, a rusticated base, empty windows; tram rails in cobbles; a spoil heap;
people and vehicles along the facade.
Reproduce: monumental stone buildings survive as blackened, pitted shells with local breaches; tram
rails flush in cobbled streets.
Use: M (Berlin act landmark).

**[10-04]** `10_later_acts_1945/10_reichstag-ruins-june-1945_iwm-bu8573.jpg`
IWM BU 8573, Sgt Hewitt (No. 5 AFPU), 3 June 1945, the Reichstag. Grade A.
Shows: the west portico with its columns pitted by shell and bullet strikes; the dome's steel frame
stripped of its glass; window openings walled up with brick, each left with two or three small
square firing slits; a demolished brick outwork in front; a sculpture group pitted with hits; a
forecourt strewn with stone lumps and metal scraps.
Reproduce: the defensive blocking of windows: openings filled with brick flush with the wall and
pierced by loopholes of about 0.15-0.25 m (+/-30 %, against the window). This is the set's only
photograph of blocked windows; whether and how they appear in the Stalingrad district must come from
R1's sources (G18). Hit pitting on stone columns is dense at the lower half. The inscription is
readable text and must not appear legibly in game.
Use: M (Berlin), G (blocked windows with loopholes), V (pitting on stone).

**[10-05]** `10_later_acts_1945/10_flag-over-reichstag-khaldei_1945-05-02.jpg`
Yevgeny Khaldei (TASS), 2 May 1945, the Reichstag roof. Grade A for the source; [staged] [print] (a
re-enactment after the building was taken; published versions retouched, smoke added).
Shows: a soldier raising a flag from the roof's sculpture line, another holding his legs; below, a
street of burnt-out blocks, smoke, vehicles and small groups of soldiers.
Reproduce: only the depth of the burnt-out city seen from a height; the smoke and tones are
retouched and the scene is iconography, not evidence.
Use: M (city depth only).

**[10-06]** `10_later_acts_1945/10_alexanderplatz-ruins-georgenkirche_barch-204-012.jpg`
Bundesarchiv Bild 204-012, Henryk Gorovits, May 1945, Alexanderplatz, Berlin. Grade A.
Shows: a broad street crowded with civilians and a truck; burnt-out five-storey blocks with empty
windows on the left; a modernist block beyond; the tall spire of St George's church; tram wires on
poles with crossarms.
Reproduce: Berlin blocks of five storeys gutted behind standing facades; tram wires; civilians
thronging a cleared road weeks after the battle.
Use: M (the Berlin act).

## 3. Visual rules

Each rule is numbered and cites the photographs it comes from. "Must" marks what a generator or a
render has to satisfy; "should" marks a strong default that the owner of the component may overrule
with a better cited source, saying so in its report. Numbers are estimates from the photographs with
the uncertainty stated in the catalogue entry they cite. Where R1's cited parameters differ from
them by more than that uncertainty, the component owner reports the conflict instead of silently
choosing. No rule sets an albedo (§1.4).

### 3.1 For G: building form

**G1. Storey pitch (floor to floor).**
- apartment_block: 3.0-3.6 m: at least 3.0 m from the balcony door in [04-11]; 3.3-3.8 m (+/-15 %)
  in [04-04]; roughly 3.0-3.7 m (+/-25 %) in [01-13] (Kharkov). The ground floor is raised about
  0.8-1.2 m above the street over a cellar, so the first balcony is 4.0-4.6 m above the ground
  ([04-11], measured on the brick courses).
- merchant_brick_house: on a plinth or semi-basement. The one measured example is low: a merchant
  house with a dentil cornice rises only about 6-7 m above the snow, a low ground floor or
  semi-basement under one main floor ([07-14], measured against soldiers standing at its foot). The
  villa's storeys read about 3.5-4.0 m (+/-20 %, from window and door proportions, [04-26]), and the
  tall windows of the larger merchant facades ([08-13], [07-16]) imply storeys of roughly 4 m. R1's
  cited heights govern.
- wooden_house: one storey; eaves 3.0-3.4 m and ridge 5.0-5.5 m above the ground, on a low masonry
  plinth ([03-04]).
- workshop: one tall storey. Brick workshops at a factory edge have piers 6-8 m high ([04-19]); yard
  sheds are lower and built of timber posts ([04-14]); blank warehouse street walls are about 3-4 m
  high ([08-12], uncertain).

**G2. Openings.** The window grammar differs by building age, not only by typology:
- 1930s apartment blocks: rectangular windows about 1.3-1.45 m wide and 1.45-1.6 m tall (window
  height about 0.44 of the storey pitch), on axes 0.8-0.95 of the pitch apart, i.e. 2.7-3.4 m
  ([04-09], measured on the frontal bays). Some 1930s buildings group tall, narrow windows (about
  0.9-1.0 m by 2.1-2.3 m) in twos and threes between wide piers ([07-15]). Balcony doors are at
  least 2.4 m tall ([04-11]).
- Pre-revolutionary brick (merchant houses, the mill, the villa): taller windows, about 1.6-1.9
  times as tall as wide (about 1.1-1.4 m by 1.9-2.2 m), often with segmental or round-arched heads
  and moulded surrounds ([04-05], [04-26], [07-14], [07-16], [03-02]).
- Ground-floor shop windows are about half the ground floor's height and nearly square, about
  2.2-2.5 m, roughly twice the height of the upper windows ([08-13], [04-10]).
- Doorways: round-arched brick doorways about 1.25 m wide and 2.1 m to the crown ([07-07], measured
  on the courses). Yard gateways are brick arches wide enough for a cart (about 2.5-3.5 m,
  uncertain) with wrought-iron gate leaves ([04-09], [07-01]). Ground-floor entrances of
  multi-storey buildings reach about 2.5 m wide ([04-12]).
- Window sills about 0.9 m above the floor ([07-12], [09-09]).
- Industrial buildings: steel window walls of small panes (0.4-0.6 m) in bands 3-5 m tall ([04-15],
  [01-14]).
- The contract's 4 m kit bay with one window per bay is sparser than any photographed 1930s block
  (axes 2.7-3.4 m). G should put the window rhythm on a sub-bay inside the 4 m bay or record the
  difference in its report; for merchant houses a 4 m bay with one window is within the range.

**G3. Walls.**
- 1930s blocks are bare face brick in regular stretcher bond with fine joints ([04-11], [07-07]) or
  rendered ([04-09], [07-15]); civic buildings are rendered ([04-10], [04-28]). Render spalls in
  patches of 0.2-1.0 m, exposing brick ([04-28], [04-09], [04-05]).
- Brick courses measure about 77-80 mm ([04-11], [07-07], [04-02]). Piers between openings on older
  and industrial buildings are 1.0-1.3 m (four to five bricks) wide ([03-03], [04-19]).
- Wall thickness at breaks: the photographs show wall tops and breaks only obliquely. The wall tops
  of the roofless one-storey houses in [07-01] read roughly as a quarter to half of a window's
  width, about 0.3-0.7 m (1.5-2.5 bricks), but no photograph gives a section, so R1's cited
  thickness governs.
- End walls are tied back with iron rods whose X-shaped anchor plates show at floor levels
  ([04-11]).

**G4. Roofs.**
- Merchant houses and one-storey brick houses: pitched or hipped sheet-iron roofs on timber rafters
  ([03-04], [07-12], [07-01]); the villa has a slate spire ([04-26]).
- 1930s blocks: flat or very low roofs behind brick parapets 0.8-1.0 m high and about 0.4-0.5 m
  thick ([07-08], [07-01], [07-16]). Chimney stacks rise above the roofline and remain standing
  2-3 m above the wall tops once the roof has burned ([04-11], [07-01]).
- City wooden houses: gabled or hipped standing-seam sheet iron at about 30 degrees, gable ends with
  fretwork bargeboards ([03-04]). Steppe villages: hipped roofs ([06-05]), reed or straw ([07-17],
  [02-10]). Steep thatch of 45-50 degrees on log houses belongs to the northern front ([01-04]), not
  the Stalingrad region.
- Workshops: gabled or segmental (arched) roofs on trusses ([04-19]); sheds with board or sheet
  roofs ([04-14]).

**G5. Cornices, parapets and string courses** define the top of a gutted shell and must be modelled
as geometry, because they carry snow and form the skyline: corbelled brick cornices ([04-05]),
dentil cornices and moulded window surrounds on merchant houses ([07-14]), balustraded parapets on
civic buildings ([04-10], [04-28]), crenellated and stepped parapets ([07-16], [07-01]), string
courses at the floor lines ([03-02], [04-01]); snow lies on all of them ([04-02], [04-28]).

**G6. Plinths, semi-basements and cellars.** Merchant buildings have rusticated ground floors over a
plinth ([08-13]); wooden houses stand on a masonry plinth ([03-04]); 1930s blocks have the ground
floor raised about 1 m over a cellar ([04-11]). Cellar headroom is 2.0-2.4 m ([04-20]). Cellar
entrances survive as brick arches about 1.2 m wide at the foot of the rubble, bridged by timbers and
planks ([07-13]).

**G7. Rooms and stairs.** Shops occupy the ground floor on the street ([08-13], [04-10]); the
concrete stair of a 1930s block (landings and flights) survives a fire that removes every timber
floor, and stands in the open when the stair bay's outer wall falls ([04-09]); cellars are furnished
from the flats above ([07-11]); sheds are timber post-and-plank structures in yards ([04-14]).

### 3.2 For G: how each typology fails

**G8. Walls stand without their floors.** Gutted brick walls of four and five storeys stand months
after every floor and roof behind them has gone, and single-bay fragments 15-20 m tall stand alone
([04-01], [04-09], [04-12], [06-12], [07-01]). The support graph must therefore let a wall stand on
the wall below it and on its cross walls; losing the floors must not by itself collapse the walls.

**G9. The floor construction decides the failure.**
- Timber-joisted floors burn out completely. The shell stands to full height, every window shows
  sky, the floors and roof lie in the cellar and ground floor, and rows of beam pockets remain along
  each floor line ([04-09], [04-01], [07-01], [07-14], [04-28], [04-10]).
- Reinforced-concrete slabs stay. They keep spanning between surviving walls, project where a wall
  has gone, or hang from their bars at a broken edge ([07-16], [04-04], [04-12], [04-24], [01-13]).
- Steel floor joists outlive their floors and project 1-2 m from a broken wall ([04-02]).
- In the blueprint, every floor therefore carries its construction (timber, reinforced concrete,
  steel). The burned condition removes timber floors and roofs entirely, as the contract says, and
  keeps concrete slabs, stairs, balconies and steel joists. A partly_collapsed building keeps a slab
  attached to its surviving support, hanging, instead of deleting it with the wall.

**G10. burned** (masonry typologies): roof and timber floors absent; perimeter walls, internal cross
walls (standing as fins), chimney breasts and stacks stand to full height, the stacks 2-3 m above
the wall tops; concrete balconies and stairs remain; window frames are gone ([04-09], [04-11],
[04-01], [07-01], [07-14], [04-28]).

**G11. shelled**: the facade keeps its window grid and many of its frames; the damage is local:
round holes of about 0.2-1.0 m through piers ([07-15], [04-02], [04-11]); breaches of 1.5-3 m whose
edges step back course by course along the bond, never smooth or round ([04-02], [07-07], [04-11]);
a collapse through the top one or two storeys at one point ([07-15]); a breached storey showing its
internal frame ([04-05]); balcony slabs with their railings torn and bent outward ([08-13]); pitting
over the whole wall (G14).

**G12. partly_collapsed**: a corner, an end or a bay down to rubble while the rest stands, with the
break following the bond and floor stubs (concrete) or beam pockets (timber) showing on the inner
faces ([04-04], [01-13], [07-16]); corner ruins keep two walls to full height ([03-02]);
free-standing fragments one bay wide can stand to full height ([04-12]); an exposed party wall
carries the ghost outline of the neighbour's floors, rooms and flues ([07-16]).

**G13. wooden_house.**
- burned: nothing stands above about 0.5 m except the brick stove and chimney on the plinth
  ([04-27], [04-07], [04-23]). A stack is 0.6-1.5 m square, often with a wider lower stage (the
  stove) and a narrower shaft under a projecting cap ([07-14]). For a one-storey house it should
  rise about 1 m above the former ridge, about 6-7 m in all ([03-04] gives the ridge; the 1 m is a
  building rule of thumb, not a measurement). Stacks of 12-13 m (+/-20 %, measured in [07-14];
  [04-27] is similar) served buildings of two or three storeys, not the one-storey wooden_house.
- shelled: boards pulled or shot away, a roof seam torn open ([03-04]).
- partly_collapsed: the roof falls in as a V of rafters while the walls stand ([04-02]), or the
  sheet roof drops onto the floor with bare rafters showing ([07-12]).

**G14. Pitting and spalling** on shelled or partly collapsed walls, and indoors where fighting went
on: fragment and bullet pits of 3-30 cm, several per square metre over whole walls ([07-07],
[04-05]), dozens per square metre where fire concentrated indoors ([04-17]); the outer half-brick
spalls in patches of 1-3 m round openings, whose edges round off ([04-05]); densest at ground floor
([08-13]). These are geometry and material state from the damage model, never decals.

**G15. Soot.** The photographs show soot as a general darkening of burned upper storeys and
interiors ([04-28], [07-01]) and as cloudy stains rising from large openings and breaches ([04-11]).
Neat plumes above every window are not visible at these print resolutions. Soot must be deposited by
the fire model where flames vented, not stamped above each window.

**G16. Roof iron.** Sheet-iron roofing tears and curls into flaps of 2-4 m that hang from rafters
and beams or lie curled on the ground and on rubble ([04-13], [03-02], [04-04], [07-01], [07-12],
[03-04]); roof frames twist ([07-01]). Burned 1930s blocks keep no roof at all.

**G17. Rubble budget.** Rubble must come from failed modules and conserve their volume, times a
bulking factor of about 1.5 (an engineering estimate, not a measurement from the photographs). The
photographs show where it goes. A gutted building's floors and roof fall inside its footprint,
filling the cellar and ground floor, and leave an external berm 1-2 m high along the base ([04-09],
[04-11], [07-16]). Walls that fall outward make slopes of 1.5-3 m against four- and five-storey
ruins ([04-04]) and mounds of 1-2 m with rods and pipes ([04-12]). Flattened one-storey houses are
mounds of 0.3-1.5 m ([04-01], [04-02]). Rubble is loose bricks with bonded lumps of 3-20 bricks
(0.2-0.6 m), beams, rods, pipes and sheet, not uniform gravel ([03-03], [04-17], [04-04]).

**G18. Defensive changes seen in the set.** Firing from windows over the sill ([07-12]), over roof
parapets ([07-08]) and over workshop sill walls ([04-19]); breaches used as passages ([07-07]);
cellars used as shelters and command posts ([04-20], [07-11]). Windows walled up with brick and
pierced by small firing slits (about 0.15-0.25 m) appear only in Berlin 1945 ([10-04]): the set has
no Stalingrad photograph of loopholes or blocked windows, so their form in the intro district must
come from R1's sources.

**G19. Interiors.** Plaster cracks in a network and falls in patches ([07-12], [09-09]); plaster is
pitted where fighting took place ([04-17]); cellars are furnished from the flats above ([07-11]).

**G20. Landmarks and civic buildings are not scaled-up typologies.** The Univermag, the station, the
elevator, the mill and the school are gutted or shelled shells with their full height and ornament
([04-10], [04-28], [04-13], [04-06], [07-03], [04-05], [07-15]). If the district includes such a
building, it needs its own blueprint.

### 3.3 For L: the district

**L1. Composition in January 1943.** The centre is a trodden snowfield with isolated standing
masses, not continuous street walls: gutted perimeter shells, free-standing wall fragments of three
to five storeys, chimney stacks, and flattened areas of low mounds, with open ground of 50-250 m
between ruins ([04-01], [06-12], [04-02], [03-02], [04-12]). Intact buildings are the rare
exception; one roofed and inhabited one-storey house appears among the ruins in February ([07-01]).

**L2. Typology mix.** Along a street: one-storey brick houses on the street line, two-storey
merchant houses with gate arches, and 1930s blocks set back in parallel rows ([07-01]). Workers'
districts: 1930s blocks in regular parallel rows ([04-08]). The fringes: burned timber quarters on a
regular grid of plots of about 8-12 m frontage ([04-07]), with stove stacks at 10-20 m pitch
([04-27]).

**L3. Widths.** Main streets are about 18-22 m between building lines ([07-01]). Squares are open
plains of 100 m or more ([07-16], [01-13]; [04-10] and [08-13] are not measurable). Broad unpaved
streets or squares on the southern fringe are 30-40 m ([04-27]). Timber-house streets are unpaved
earth ([03-04]). Pocket and village roads are 6-8 m ([05-01], [01-10]). A Tiger (3.7 m) fills about
half of a 7-8 m road ([01-10]). In an 18-22 m street, berms 1-2 m high (G17) lying at their angle of
repose run 2-4 m out from each wall (an estimate), which leaves a clear lane of at least 10 m.

**L4. Street furniture.** Tram tracks with rails flush with or slightly proud of the surface
([08-12], [10-03]); timber poles 7-9 m high with one to four crossarms ([04-14], [08-12]), some
braced by raking struts ([06-01]), many leaning or broken off ([04-04], [08-13]); wires sagging
across streets and hanging broken ([04-04], [07-14]); lamp brackets on walls ([04-04]) and lamps on
poles ([04-05]); globe lamp posts on squares ([07-16], [08-13]).

**L5. Fences and garden walls.** Picket fences of 0.8-1.1 m and board fences of 1.6-2.0 m in the
timber quarters, with boards missing ([03-04], [04-05]); garden walls of brick piers with pipe
rails, 0.6-1.0 m, round squares ([04-10]); wattle fences of 1.5-1.8 m in villages ([07-17],
[02-04]).

**L6. Trees.** Stripped to trunks and main limbs, 10-15 m tall, on squares and streets ([04-10],
[07-14]); broken rather than burned ([04-02]); charred stumps ([04-01]); garden shrubs broken to
0.5-1.5 m ([04-23]).

**L7. Rubble and dressing.** Berms and mounds follow G17. Dressing lies on top of the snow: sheet,
planks, beams, rods, pipes, wire and rags ([04-01], [04-04], [04-11], [03-02]).

**L8. Snow in January 1943.** 5-15 cm on open ground in the centre, trodden to a crust, with brick
edges and debris showing through ([04-01], [04-11], [06-12], [07-14]); 20-30 cm on squares
([08-13]); 0.3-0.4 m in factory yards ([04-25]); drifted smooth over rubble inside roofless
buildings, 0.3-0.8 m deep ([07-10], [07-01]); on every ledge, wall top and cornice ([04-02],
[04-28]); 0.2-0.4 m drifted on flat roofs ([07-08]). February is thinner and wind-scoured, with bare
patches ([04-06], [04-10]); in March the ground is dark and wet ([07-15]).

**L9. Paths and traffic.** Trodden paths go dark and compacted; vehicle routes are cut to earth,
brown slush or mud in the ruts ([04-23], [02-08], [01-10], [06-03]); footpaths through deeper snow
are trodden to mid-calf ([04-25]).

**L10. Obstacles and field works.** Hedgehogs of crossed rails and I-beams ([04-19]); barbed wire on
stakes ([04-04], [04-21]); trenches through squares and gardens with spoil ridges of 0.5-1 m
([04-10], [03-14]); log revetments ([01-07]); lifted mines laid in rows ([02-07]). The set has no
photograph of a Stalingrad street barricade of rubble and vehicles, and few of craters in the city
([04-11] a shallow crater in snow; [04-08], [04-16] cratered yards from the air); both must follow
R1's sources.

**L11. Wrecks.** Knocked-out tanks stand in streets and squares ([08-12], [08-13]), each inside a
debris halo of 5-15 m ([06-08], [01-25]); vehicles are abandoned along one side of a road, leaving
the other side passable ([05-01]); wrecks lie at ditch and gully edges ([08-14], [01-30]).

**L12. Yards and the factory edge.** Gateways from the street ([04-09], [07-01]); sheds of timber
posts with roofs caving inward ([04-14]); machine tools, steel vessels, wagons and wheelsets as
cover ([04-15], [03-03], [04-25], [04-14]); workshop sill walls ([04-19]); ammunition and
projectiles stacked loose in heaps against walls ([09-05]).

**L13. Slopes inside the city.** Balkas and the Volga bank are cut with dugouts: plank-framed
doorways 1.2-2.0 m tall with dark spoil fans below them ([04-26], [04-21], [04-22], [06-13],
[09-07]).

**L14. The open winter ground around the district**: flat, trodden snowfields with weeds standing
through thin snow ([02-01], [03-10], [04-23]); streets surviving only as tracks across the snow
([04-23]); the edge of the centre opens onto one-storey quarters ([04-02]). See also M1.

### 3.4 For S: physical traces of human events

**S1. A fire in a wall's lee**: a small fire on the ground, cooking pots, crates as seats, trodden
snow and dark scraps, in the lee of a large standing wall or under a surviving overhang ([04-11],
[06-12]).

**S2. Firing positions indoors and on roofs**: fire from the sill of a window with crates and
benches, spent cases on the floor, cracked plaster, and a view down onto collapsed roofs ([07-12],
[04-17]); on a flat roof, behind the parapet ([07-08]).

**S3. Anti-tank rifle and gun positions**: cartridges laid out on a cloth or board, spent cases on
the snow, straw, boxes ([03-03], [03-16], [06-09], [02-13]); a gun in a shallow pit behind a low
parapet with its shield top at the height of the surrounding cover ([03-07], [03-10]); ground cover
blown out round the muzzle ([03-01]); guns wheeled out of an earth-covered log bunker by a snow ramp
([03-09]). Riflemen's positions: a trench about 1 m deep with a dugout opening off it ([03-14]), or
a scrape under a shelter-half with straw and grenades laid ready ([06-10], Moscow 1941: an attested
practice, not a Stalingrad record).

**S4. Cellar shelters and command posts**: a tarpaulin stretched under the ceiling, a shell-case
lamp, a field telephone with cables along the floor, a table of boards ([04-20]); a samovar,
bedsteads, bedrolls and blankets hung as screens ([07-11]); a sheet-metal stove with a spliced pipe
([09-06]); a signals post in a bare room with a set, a barrel and a blanket ([09-09]). Telephone
cables run along floors and lie on the snow between positions ([04-20], [09-08]). The light is the
lamp, the stove or the window, modelled as emitters or openings; the flash of the prints is not the
room's light.

**S5. Dugouts in slopes**: plank-framed entrances, spoil fans, and furniture and bedsteads carried
down from the flats ([04-22], [04-21], [09-07]).

**S6. Abandonment**: kit dropped on top of the last snow (greatcoats, blankets, boxes, a rifle,
planks, paper, office objects) ([05-01], [05-02], [08-14]); stores collected in rows ([05-03],
[05-07]).

**S7. Knocked-out tanks**: a halo of dropped kit, planks and track links within 5-15 m, with the
broken track folded beside the hull ([06-08], [01-25]); spent cases and boxes beside a tank that
fought from a halt ([02-13]).

**S8. Graves**: rows of low mounds, about 2.0 x 0.8 m and 0.1-0.2 m high, with steel helmets as
markers and a few plain wooden markers, a narrow path between them, no text ([05-12]).

**S9. The identification tag** broken along its perforation is an attested trace of a death
([09-13]); its stamped markings stay unreadable.

**S10. Signs and slogans**: hand-painted street signs, slogan boards and turret slogans are attested
([04-23], [04-10], [02-12]), but readable text breaks the first principle: at game resolution they
must stay illegible, or be left out.

**S11. People**, only where a tableau needs them: Soviet soldiers in white oversuits, greatcoats,
quilted jackets, long sheepskin coats, ushankas and felt boots ([04-01], [07-07], [07-09], [06-14],
[02-09]); German prisoners in greatcoats with blankets over their heads, scarves and improvised foot
wrappings, never in white clothing ([07-04], [07-02], [06-13]); civilians in headscarves, dark coats
and felt boots, pulling small sledges ([07-14], [07-15]). The German padded reversible suits and
whitewashed helmets in [06-06], [06-07], [01-11] and [09-11] are photographed in the winter of
1943-44, and [07-13] is an unverified book scan: none of them may dress the Stalingrad garrison of
January 1943 without a source. Postures of crews in action: [03-01], [03-06], [03-17], [05-16].

**S12. Bodies.** The set deliberately excludes almost every image of the dead. The one exception
([02-13]) shows a crewman slumped face down over a hatch rim: small in the frame, still, clothed, no
wounds shown. A tableau that uses an attested death must be at least that restrained; grave rows
([05-12]) and the broken tag ([09-13]) show death as aftermath and are preferred.

**S13. Photographs of specific events date and place conditions; they are not tableaux.** No scene
may imply that the player's crew witnessed Paulus's surrender ([07-06]), the prisoner columns
([07-01], [07-02]) or any other photographed moment.

**S14. The crew's own traces at a halt**, for the player's Tiger and for the place where it last
stood: ammunition crates stacked beside the hull and rounds handed up one at a time ([09-03],
[09-01]); rounds laid across the front plate before stowage ([09-02]); fuel drums with a hand pump
and hose ([09-04]); clothing, mess tins and tools spread on the engine deck ([01-05]); cut branches
heaped against hull and running gear ([01-06]). These are photographed in 1943-44 away from
Stalingrad: they show crew practice, not a Stalingrad event.

### 3.5 For V: what a render must match

**V1. Compare at the game's resolution.** Downsample each named photograph to the internal
resolution (640x360, greyscale) and compare silhouette, massing, skyline and the order of values;
never texture detail or absolute grey levels. At that size [04-01] still shows its isolated masses,
stacks and haze, and [07-01] its street width and typology mix: these are what survive the crush and
what a render must get right.

**V2. Overcast**: snow ground and sky nearly the same value, the horizon lost, vehicles and
buildings the only dark masses, no cast shadows ([06-02], [04-01], [04-25]).

**V3. Clear, low winter sun**: sunlit snow is the brightest surface in the frame; shadows on snow
stay mid-grey, clearly lighter than dark cloth and never black; shadows are long ([07-10], [03-09],
[02-13], [07-14], [06-12]). The prints cannot show the shadows' blue cast; it must come from the sky
light in the transport, not from a grade.

**V4. Distance.** Contrast falls with distance through the participating medium: at 100-200 m in the
factory district under overcast ([04-25]); at 200-400 m in the centre ([04-01]); buildings beyond
about 500 m barely separate from the sky ([07-01]); in a ground blizzard only silhouettes remain
beyond a few tens of metres ([06-11]); falling snow shows as short streaks near the camera and greys
everything at 100-200 m ([08-07]). This must come from extinction and scattering in the medium, not
a screen-space veil (CLAUDE.md, second principle).

**V5. Gutted shells show sky.** Every window of a burned building shows sky, including the far
wall's windows seen through the empty interior ([04-01], [04-09], [07-01], [04-28]). Dark interiors
behind burned windows mean floors or roofs that should not be there.

**V6. Interiors against exteriors**: a dark interior beside bright snow outside ([06-12]);
window-lit rooms with a steep fall-off ([09-09], [07-12]); inside armour, the vision blocks are the
brightest thing and the rest is near black ([01-16], [09-10]). No fill light may raise these
interiors.

**V7. Pitting density** on shelled masonry ([04-05], [07-07]) and interior plaster ([04-17]).

**V8. The player Tiger**: side profile ([01-08]); front ([01-27], [01-25]); early-production
silhouette and head-on view ([01-01], [01-02]); the drum cupola ([01-15]); hatch openings and rounds
([01-17], [09-01]); turret and basket ([01-19]); engine bay and deck ([01-20], [01-21]); hull tub
([01-23], [01-24]); track and running gear ([01-29], [01-14]); human scale ([01-26]); the 1942-43
blotchy, hand-brushed whitewash ([01-04], [01-03], [01-09]) and its wear ([01-07], [01-11]); snow
packed in the track links ([01-12], [06-01]); scale against ruined multi-storey blocks ([01-13]);
hit marks as craters and gouges ([08-01], [08-02], [08-03], [01-28]); end states: engine deck blown
open with the turret on ([01-30]), turret thrown off ([08-10]).

**V9. Soviet vehicles**: a T-34 knocked out in a street ([08-12]); without tracks ([01-03]); burning
([08-11]); intact but abandoned ([08-16]); blown apart ([08-17]); T-34 geometry and turrets
([02-14], [02-17], [02-08]); markings brushed on by hand ([02-11]); camouflage patterns ([02-06]);
T-60 and T-70 scale ([02-03], [02-18], [02-19]).

**V10. Smoke**: backlit smoke haze with the sun a pale disc ([04-12]); dense white smoke inside a
hall ([04-18]); a smoke column over a square ([07-16]).

**V11. Snow placement**: on horizontal surfaces and ledges, not on vertical faces ([04-02], [04-28],
[05-04]); caked on the upper edges of guns and tyres while the lower faces stay dark ([03-08]);
drifted in the lee and scoured on the windward side ([05-13], [05-06]); packed into track links
([01-12]).

**V12. Skyline**: the ruined centre's skyline is ragged: stepped wall tops, stacks and fragments,
cornices surviving in pieces ([04-01], [06-12], [03-02], [07-16]).

**V13. Bursts and blasts on snow**: a shell burst on snow throws a dark column of earth and snow
several metres high and leaves a dark fan of earth on the white ([03-13]); a gun's muzzle blast
raises the ground cover for several metres to both sides ([03-01]); thrown earth lies as dark clods
on snow round wrecks ([08-14]). These are particles and deposits from the simulation, not sprites or
decals.

### 3.6 For the maps beyond the intro district

The open winter ground around the district:

**M1.** Flat, trodden snowfields with weeds through 5-15 cm of snow, dark tracks and paths, a few
poles and fence lines, and the city's ruins on the skyline ([04-01], [04-23], [02-01]).

The steppe and the pocket on the route west (January 1943):

**M2. Relief**: long gentle slopes with 10-30 m of relief over 1-2 km and sightlines of kilometres
([05-15], [02-20], [02-21]); balkas cut into it with convex shoulders, shrub-filled gullies and
near-vertical loess faces of 4-5 m ([02-21], [05-14]).

**M3. Snow on relief**: it lies in hollows and on lee slopes and is scoured off convex ground in
dark streaks that follow the contours ([05-06]); 15-30 cm on the open steppe with grass stems
through it ([05-02], [05-11]); drifts of 1.0-1.5 m against obstacles and in cuttings, with scour
hollows on the windward side ([05-13]); roads as packed layers of 5-10 cm with ruts ([05-01],
[05-17]) or troughs between banks of 1-1.5 m ([07-02]). Deep powder that buries running gear to the
wheel tops ([06-04], winter 1941-42) is the extreme case only.

**M4. Frost**: rime on the windward side of cold metal ([05-11]); hoarfrost on shrub stems
([03-15]); rime on wool and fur ([07-05]).

**M5. The pocket's debris**: abandoned vehicle columns along one side of balka roads ([05-01]);
dense vehicle parks with 1-2 m gaps ([05-04]); abandoned batteries tens of metres apart behind low
snow parapets ([05-05]); rocket launchers and burned trucks on valley floors ([05-06]); air-supply
containers collected in rows ([05-07]); airfields as flat, open snow plains with aircraft parked
close together ([05-08], [05-09]); shot-down aircraft ([05-10]); railhead dumps ([05-03]); war
graves by villages ([05-12]); wrecks at gully edges ([08-14]).

**M6. Villages on the route west**: one-storey houses with hipped roofs on open snow plains
([06-05]); low timber and clay houses and sheds of poles ([07-04]); wattle fences and whitewashed
clay walls ([07-17]); steel wind pumps at the village edge ([08-15]); log houses with straw roofs
([02-10]); a gutted two-storey brick building standing as a single wall ([02-01]). The thatched log
villages ([01-04]) and pine woods ([01-10]) of the northern front are not the Stalingrad steppe.

**M7. Weather**: overcast with the horizon lost ([06-02]); blowing snow with visibility of tens of
metres ([06-11], [02-02]); low winter sun from the side ([05-17], [02-03]).

Later acts:

**M8. The steppe in summer and autumn, 1942 and 1943**: pale dry grass, wide valleys and telegraph
lines ([02-20], [09-12], [05-15]); steppe fires ([05-15]); villages of whitewashed thatched cottages
with maize and poplars ([08-05], [08-06]); guns hidden under cut branches and grass ([03-05],
[03-07], [03-11]); disabled Tigers towed by paired tractors ([08-04]); field repair in the open,
with gantry cranes and crane trucks ([01-18], [01-22], [08-09]); thaw mud in rear-area workshops
([08-08], [03-12]); tanks concealed inside deciduous woods ([02-16]) and waiting at a wood edge
([02-05]); tanks 30-60 m apart in the open with infantry among them ([02-15]).

**M9. 1945**: the Oder plain's flat farmland with village tree lines, and gun lines at 20-30 m
intervals ([10-02]); observation posts under nets ([10-01]); Berlin's five-storey blocks gutted
behind standing facades, with tram wires and crowds ([10-06]); the Reichstag's pitted stone,
bricked-up windows with firing slits and bare dome frame ([10-03], [10-04]); [10-05] for the city's
depth only.

**M10. Shattered forest**: branchless, splintered trunks of 10-20 m after sustained shelling
([01-28]).

**M11. The northern factory districts**, if a later act or the route reaches them: steel-framed
halls fail roof first, their cladding and purlins gone and their trusses sagging between columns
that still carry the crane girders ([04-03], [04-18]); snow lies inside on floors and machines
([04-03], [07-10]); steel window walls lose their glass but keep their grids ([04-15]); from the
air, roofs burn through along the ridge and halls read as combs of framing inside standing perimeter
walls, beside settlements of 1930s blocks in parallel rows ([04-16], [04-08]); multi-storey shop
blocks hang their floors at the openings under a standing chimney ([04-24]).

## 4. Photographs to compare renders against

Each row names the photographs a render of that subject should be put beside and what to check
first. The boards `photo_boards/compare_typologies.jpg`, `compare_environments_district.jpg` and
`compare_environments_beyond.jpg` lay the rows out in this order, each with a render slot.

| Subject | Compare against | Check first |
|---|---|---|
| merchant_brick_house, form | [08-13], [07-14], [07-16] | rusticated ground floor, tall windows with moulded heads, cornice |
| merchant_brick_house, burned | [07-14], [07-01], [04-01] | roofless shell to the cornice, sky in every window |
| merchant_brick_house, shelled | [08-13], [04-05] | pitting densest at ground floor, torn balcony railing |
| merchant_brick_house, partly_collapsed | [03-02], [04-12] | corner keeping two walls, ragged stepped top |
| apartment_block, form | [04-09], [04-11], [07-15] | window grid proportions, raised ground floor, stair bay |
| apartment_block, burned | [04-09], [04-11], [07-01] | full-height shell, concrete stair standing, stacks above walls |
| apartment_block, shelled | [07-15], [04-11], [01-13] | grid intact, local holes, stepped breach at a corner |
| apartment_block, partly_collapsed | [07-16], [04-04], [04-12] | slabs projecting or hanging, floor stubs, lone fragment |
| wooden_house, form | [03-04], [06-05], [07-06] | one storey on a plinth, 30 degree sheet roof, fences |
| wooden_house, burned | [04-27], [04-07], [04-23] | stove and chimney only, at plot pitch |
| wooden_house, shelled / partly_collapsed | [03-04], [04-02], [07-12] | missing boards, V-collapsed rafters |
| workshop, all conditions | [04-19], [08-12], [04-14] | pier and bay, tall single storey, caving shed roofs |
| district overview (elevated) | [04-01], [04-02], [07-01] | isolated masses on snow, skyline, haze with distance |
| main street | [07-01], [08-12], [04-04] | width, typology mix, rubble berms, wires |
| central square | [04-10], [07-16], [08-13] | open plain, trenches, stripped trees, lamp posts |
| open ground between ruins | [06-12], [04-11], [03-02] | 50-250 m of trodden snow, wall fragments |
| burned timber fringe | [04-27], [04-23], [04-07] | stacks in rows, streets as tracks |
| factory edge and yards | [04-25], [04-15], [03-03] | snow over clutter, machines and vessels, haze |
| railway edge | [04-14], [04-07] | wheelsets, poles with crossarms, rail corridors |
| station and fountain | [04-28], [04-13] | shell facade, torn roof iron inside |
| grain elevator | [04-06], [07-03] | concrete mass holed but standing |
| cellar interior | [04-20], [07-11], [09-06] | low ceiling, lamp and stove as the only light |
| balka and Volga bank | [04-21], [04-22], [04-26] | steep slope, dugout doors, spoil fans |
| open winter ground round the district | [04-23], [02-01], [03-10] | thin snow with weeds, tracks, fences |
| steppe route west in winter | [05-05], [05-17], [06-02] | open plain, lost horizon, sparse verticals |
| balka roads in the pocket | [05-06], [05-01], [08-14] | scoured slopes, abandoned columns |
| drifts | [05-13], [06-04] | lee drifts and windward scour |
| villages on the route west | [06-05], [08-15], [07-04] | hipped roofs, open spacing, wind pump |
| airfield | [05-09], [05-08] | flat snow plain, aircraft as the only verticals |
| blizzard and snowfall | [06-11], [08-07], [02-02] | contrast loss with distance |
| player Tiger | [01-08], [01-27], [01-04] | silhouette, whitewash, snow in tracks |
| player Tiger in a city | [01-13], [01-12] | scale against ruined blocks, snow in the links |
| later acts: summer steppe | [02-20], [02-21], [09-12] | pale grass, wide valleys, balka shoulders |
| later acts: Kursk villages | [08-06], [08-05] | thatch, maize, poplars, mud at crossings |
| later acts: Oder and Berlin | [10-02], [10-04], [10-06] | open farmland gun lines; gutted stone and brick |

## 5. What the set cannot tell

- **Colour in winter.** All seven colour slides are summer or autumn 1942 (§1.4); winter albedo must
  come from physical data.
- **A Tiger near Stalingrad.** None exists in the set (README, gaps); the Tiger photographs set the
  vehicle, not the place.
- **Loopholes, blocked windows and street barricades in Stalingrad.** Not shown (G18, L10).
- **Wall sections.** Wall thickness is not measurable from these photographs (G3).
- **Craters in the city.** Only one crater in snow and two aerial views (L10).
- **Pavlov's House and Mamayev Kurgan.** Pavlov's House is link-only; there is no landscape of the
  Kurgan (README, gaps).
- **The Tiger's interior.** Only a doubtful interior ([01-16]) and a different vehicle ([09-10]).
- **Dates of the Strunnikov series.** Most of folder 05 is dated only "1943"; the snow depths they
  give apply to January-February 1943 with that caveat.
