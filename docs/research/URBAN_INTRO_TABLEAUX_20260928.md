# Urban intro tableaux: eleven wordless scenes for the Stalingrad city centre, winter 1942–43

Role R2 (tableaux research), urban-intro city team, 28 September 2026.
Readers: S (story), L (district), G (grammar), V (verify), I (integrator).
Builds on `assets/worldgen/scene_blocks/historical_scene_catalog.v1.json` (36 scenes),
`docs/ACT_ONE_HISTORICAL_SCENES.md` and `docs/ENVSTORY.md`. None of the scenes below repeats one of
those entries. Section 6 lists where they touch, and corrects three of them.

## 1. Build order

| Rank | Id | Title | Class | Host (typology · rooms) | Depends on |
|---:|---|---|---|---|---|
| 1 | `a1_cellar_embrasure_two_layers` | The low window | attested_condition | merchant_brick_house · cellar, street | cellar windows in the plinth; a wrecked German tank |
| 2 | `a1_red_cross_cellar` | The red cross over the cellar stair | district_event | merchant_brick_house or apartment_block · cellar, stair, yard | a reachable cellar (outdoor version exists) |
| 3 | `a1_chimney_field_dugout` | Smoke from the ground | attested_condition | wooden_house (burned) · yard | the grammar's burned wooden house: stove and chimney only |
| 4 | `a1_tram_on_its_rails` | The tram that stopped on 23 August | attested_condition | street in front of any typology | L's tram rails; a new tram-car asset |
| 5 | `a1_white_canopy_canister` | White canopies | attested_condition | apartment_block · yard or square | canister and canopy assets |
| 6 | `a1_basement_with_stovepipe` | The basement with the stovepipe | district_event | small merchant_brick_house or wooden_house · cellar, yard | a reachable domestic cellar |
| 7 | `a1_floor_breach_strongpoint` | Through the floor | attested_condition | apartment_block · stairwell, dwelling, cellar | slab failure in the structural damage model |
| 8 | `a1_kitchen_firing_post` | The kitchen chair at the shell hole | attested_condition | apartment_block · upper-floor dwelling, stairwell | walkable upper floors, or readable from the street |
| 9 | `a1_evicted_household_yard` | Turned out into the yard | attested_condition | wooden_house or merchant_brick_house (roofed) · yard, dwelling | one of the few roofed houses |
| 10 | `a1_yard_of_the_unburied` | Where the ground would not open | attested_condition | apartment_block or merchant_brick_house · yard | best placed as the yard of rank 2 |
| 11 | `a1_residents_barricade` | The street the residents closed | attested_condition | street | a photograph of a Stalingrad barricade; none is in the set |

**Build first: ranks 1–5.** If S builds only three, build ranks 1, 2 and 3. Together they cover the
four typologies and both layers, and they teach the intro's lessons in order:

- **Rank 1:** this city shoots from its cellars.
- **Rank 2:** your army is dying underground.
- **Rank 3:** its people live in the ground.
- **Rank 4:** it was an ordinary city until one afternoon.
- **Rank 5:** the air bridge is failing, and open ground kills.

If F2 delivers enterable cellars, rank 6 is the strongest replacement for rank 5. If cellars stay
unreachable, build rank 2 in its outdoor version (§4.2) and do not start rank 6.

Suggested order along the route from spawn (1850, 250) westward (L decides the geometry):

1. The tram on the spawn street, with the canopy on the skyline behind it.
2. The embrasure at the first corner.
3. The red-cross cellar in a side yard, with the unburied (rank 10) against its yard wall.
4. The chimney field on the timber fringe toward the west exit.

## 2. Scope, dates and method

### 2.1 Time frame and the two layers

The target is the city centre from September 1942 to 2 February 1943. Every scene is depicted in
**20–31 January 1943**, when the German-held centre became the southern pocket. On 26 January the
pocket was split. The southern pocket surrendered on 31 January and the northern on 2 February.
These dates come from Adam and Ruhle as summarised in the
[English Wikipedia article](https://en.wikipedia.org/wiki/Battle_of_Stalingrad).

Each scene carries two layers, as ACT1_MAP.md describes:

- **The autumn layer (September–November 1942: "your" advance).** It appears only as old evidence:
  - weathered char and soot with frost on it;
  - wrecks capped with snow;
  - graves from the autumn.

  Nothing from September burns in January. This follows the 25 September source audit.
- **The winter layer (January 1943).** It is fresh: the besieged army's collapse and the Soviet
  counter-offensive.

### 2.2 Where the district's ground really lies

`tools/worldgen/gen_world.py` builds the Act 1 terrain from a window of the Stalingrad DEM:

- The window is centred at 48.708 N, 44.485776 E (lines 34–37). That is 2,000 m west of the DEM
  centre at 48.708 N, 44.513 E (`assets/terrain/stalingrad_100km_meta.json`).
- +X is east and +Z is south (`src/terrain.rs:4`).

The intro district spans x 1450…2050 and z −100…450. On the ground that is about
**44.5055–44.5137 E, 48.7040–48.7089 N.** Its neighbours, from `docs/research/data/stalingrad.json`:

- **The main station (Stalingrad-I)** maps to about (2045, −500), 400 m north of the district.
- **The Univermag** maps to about (2258, −67). That is 200 m beyond the district's east edge and just
  outside the act's +2,200 m bound.

The district's real terrain is therefore the southern city centre, south-west of the station and
west of the Square of Fallen Fighters:

- The German 71st Infantry Division took it in September 1942.
- On 28–31 January 1943 the 64th Army cleared it. Its 38th Motor Rifle Brigade crossed the Tsaritsa
  with the 36th Guards Rifle Division, then fought through the candy factory, the gardens by the City
  Theater, the Regional Party Committee buildings and the Univermag (Hellbeck, "The Capture of Field
  Marshal Paulus").

This makes `district_event` scenes from that fight valid in the intro district.

This alignment does not make any building here a real one. The act's landmark pylons are not at
their real positions: the station pylon is at (−900, −300). Do not use this alignment to place an
`exact_landmark` unless the owner declares the building a campaign-scale representation of that
landmark.

### 2.3 How the research was done

**How sources were reached.** The session's web-search quota was already spent when this task
started, so no search-engine queries were made. Sources were fetched directly:

- **Wikipedia and Wikimedia Commons,** through their APIs.
- **Two books,** in full text on erenow.org. These are unofficial access copies, used only to locate
  passages; metadata should cite the printed editions:
  - Jochen Hellbeck, *Stalingrad: The City That Defeated the Third Reich* (PublicAffairs, 2015),
    the English edition of the 1943–44 Soviet historical-commission interviews. The Russian edition
    is [НЛО 2015](https://www.nlobooks.ru/books/biblioteka_zhurnala_neprikosnovennyy_zapas/19217/).
  - Antony Beevor, *Stalingrad* (Viking, 1998), together with its source notes.
- **The Stalingrad Battle Museum-Reserve documentary site.**
- **The local reference set.**

**Photographs viewed.** I looked at 12 photographs from the set and at three Commons photographs
outside it. I viewed the three outside ones from temporary files and then deleted them.

**Source classes and how far to trust them:**

| Class | Examples | Use |
|---|---|---|
| Soviet commission interviews, January 1943 – January 1944, state-organised, speakers named | Duka, Morozov, Yegorov, Rodimtsev, Aksyonov, Pozdnyakova | Material arrangements and dates. The Soviet framing is recorded in metadata. Some speakers describe killing prisoners and wounded; that is recorded, never staged. |
| Captive interrogation | Sergeant Helmut Pist, 9 February 1943 | Conditions only; coercive context |
| German wartime documents, cited by Beevor with archive numbers | BA-MA RH19 VI/12; RW4/v.264; ÖStA-KA B/1540 | Contemporary. Reached through Beevor's notes; not re-inspected here. |
| German retrospective accounts | Haidinger 1992, Achleitner, Gebele, Dibold 1949, Behr 1995 | Conditions, not figures |
| Wartime photographs | TASS or Museum-Reserve; German PK | Form and damage, never albedo. Captions are often propaganda. |
| Soviet-era account of Pavlov's House | Gurkin 1963, through ru.wikipedia | Physical arrangements only. Hellbeck notes the episode was later mythologised. |
| Secondary works | Beevor 1998; Wikipedia | Chronology, and to locate primary sources |

### 2.4 Rules for every scene

1. **No readable text.** Tram route boards, canister stencils, ammunition and tag markings, maps,
   papers, notices, slogan boards and chalked messages are blank or illegible marks. The red cross is
   a non-verbal emblem and is allowed. Do not use Nazi or Soviet flags and insignia as decoration.
2. **No figures from the sources.** How many bodies, beds, canisters or cases appear is an authoring
   choice, and the metadata records it as one.
3. **Light only from modelled emitters** (CLAUDE.md, second principle):
   - daylight through real openings;
   - stoves, tin wick lamps, a pressure lamp, vehicle headlamps on a battery.

   No fill light. A dark cellar stays dark between its lamps.
4. **Sound only from modelled sources, propagated through the rooms:** wind at openings, stoves,
   lamps, cloth, wires, distant weapons. No non-diegetic sting and no spoken word. Coughing and
   breathing are allowed; words are not.
5. **People.**
   - Living people are not static props: a scene shows who lived there through what they left.
   - The dead are few, covered and faceless, and never posed into a story.
6. **Metadata fields for each scene**, as the 25 September source audit recommends:
   `event_date_or_window`, `depicted_date_or_window`, `place_envelope`, `source_url`,
   `documented_relationships`, `inferred_geometry_and_props`. Each scene's "Inferred" line below
   lists what belongs in the last field.

## 3. The candidate subjects: what is attested

| Candidate | Verdict | Attested (who, where, when) | Physical traces | Used in |
|---|---|---|---|---|
| Cellars as shelters and aid posts | **Attested** | August 1942: "Cellars were packed with hundreds of people" (Major Demchenko, city commandant). 20–31 January 1943: about 20,000 wounded in cellars under the ruins (Beevor, ch. 22). Specific cellars: 600 badly wounded in the theatre cellars "with no light and no sanitation" (Dr Haidinger). A dressing station under the NKVD building with a red-cross flag and a gas lamp (Dr Dibold). The Univermag basement "was their hospital", about fifty wounded, sick and frostbitten (Major Yegorov). A German hospital in the candy-factory basement (Captain Morozov). On 24 January Paulus reported "18,000 wounded without the slightest aid of bandages and medicines". | Red-cross flag; improvised lamps; straw and blankets; paper bandages; the dead piled outside | Ranks 2, 6, 10 |
| Civilians living in cellars and dugouts | **Attested** | People hid in "shelters, basements, trenches" from 23 to 28 August (Pigalyov). Children were found in "courtyards, apartments, trenches, and basements" (Vodolagin). About 15,000 civilians were in German-held areas at the end of November (Hellbeck). A census after the battle found "at least 9,796 civilians", including 994 children (RTsKhIDNI 17/8/226, through Beevor). "Most of the children had been living in the ground for four or five winter months" (an American aid worker, through Beevor). Pozdnyakova's family lived in courtyard trenches and cellars. | Yard trenches roofed with doors; stovepipes; sledges; salvaged goods | Ranks 3, 6, 9 |
| The frozen dead that could not be buried | **Attested** | At Pitomnik, "piles of frozen corpses, left by the field hospital there because the ground was frozen too hard to bury them" (Morzik, through Beevor). Piles of frozen corpses outside the Tsaritsa tunnels (Dr Achleitner). "Thousands of dead bodies, and the wounded men dying in the streets" (Pist). In autumn the Germans still buried their dead (Beevor, ch. 12). | Piles or rows under blankets; broken identity-tag halves; boots taken | Rank 10, beside rank 2 |
| Horses slaughtered for food | **Attested; already covered** | Pozdnyakova ("they left us … only hooves and guts"). "Almost all the horses had been eaten" by 17 January (Beevor, ch. 21). A German prisoner: "the horses have all been eaten" (Hellbeck, ch. 32). Yefim Kopyt photographed a dead horse beside a tram in February 1943. | Carcasses; hacked flanks; saws | Covered by `a1_horse_road_hospital`, `a1_red_army_house_frozen_courtyard` and ENVSTORY's "Frozen Horses". Here only as the photographed carcass (rank 4) and a traded lump of frozen horsemeat (rank 6). |
| Air-drop containers | **Attested** | After 23 January there were no landings, only drops (Adam and Ruhle). The canopies stayed white despite Sixth Army's requests for red (Behr, through Beevor). Men retrieving canisters in the open were picked off by snipers (Beevor, ch. 22). Air-dropped food "was being taken by whoever found them, despite strict orders and the threat of being shot" (Pist). | White canopies on ruins; long cylindrical canisters | Rank 5 |
| Pavlov's House | **Attested; exact landmark, not placed** | 42nd Guards Rifle Regiment on 9 January Square, 23 September – 25 November 1942. Narrow embrasures, mined approaches, wire, a deep communication trench, a buried telephone line; more than thirty civilians in the basement (ru.wikipedia, citing Gurkin 1963; Beevor, ch. 12). | See the previous cell | Its physical practices are used in ranks 1 and 7; no replica |
| The L-shaped house and the railway workers' house (9 January Square) | **Attested; exact landmarks, not placed** | Both were German strongpoints, stormed from 3 December 1942. The attackers dug saps to within 20–30 m, broke through the floor above a cellar with crowbars and used flamethrowers. Under the railway workers' house they dug a 50 m tunnel with 3 t of TNT, leaving a crater 60 m across (Rodimtsev interview). | Saps; floor breaches; soot; crater | Rank 7, as an attested condition |
| A second L-shaped building, in the southern centre | **Attested** | German-held. On 29–30 January 1943 it had "heavy machine guns in the basements, cut embrasures in the walls. You couldn't see any of it from outside" (Morozov). | Basement embrasures | Ranks 1 and 7 |
| Fire-gutted blocks | **Attested** | Luftflotte 4's raid on 23 August 1942. Fires could not be fought because the water supply was cut (Denisova, Ioffe). On 17 September Pozdnyakova's block burned. | Masonry shells without floors; soot above openings; timber quarters reduced to stoves and chimneys | The setting for ranks 3, 7 and 8; otherwise the grammar's `burned` condition |
| The Univermag | **Attested; exact landmark, not placed** | September 1942: headquarters of a Soviet battalion. From late January 1943: Sixth Army headquarters in the basement, a hospital (Yegorov), a courtyard "mined all around" (Morozov), corridors used as latrines (Soldatov). Its true position lies outside the act map (§2.2). | — | Corrections in §6 |
| Stranded tram cars | **Attested** | Trams ran until 23 August 1942 ([ru.wikipedia](https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%B0%D0%BC%D0%B2%D0%B0%D0%B9); Demchenko: "even to get the trams running"). Kopyt photographed a riddled car in a central street in February 1943. | Kh-type two-axle car on 1,524 mm rails; brackets and span wires | Rank 4 |
| Trolleybuses | **Refuted** | Stalingrad's trolleybus system opened on 31 December 1960 ([ru.wikipedia](https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%BE%D0%BB%D0%BB%D0%B5%D0%B9%D0%B1%D1%83%D1%81)). | None. Never show a trolleybus or twin trolley wires. | — |
| The station fountain | **Attested; exact landmark, covered** | ENVSTORY, and the Act 1 pylon at (300, −150). "About the only landmark left standing" (Beevor, ch. 22). | — | Not duplicated |
| The grain elevator | **Attested; exact landmark, covered** | `a1_grain_elevator_extraction_convoy` and ENVSTORY | — | Not duplicated |
| Ruined schools | **Partly attested** | School No. 38 near 9 January Square was a German strongpoint, stormed in December 1942 (Rodimtsev). The contract has no school typology. | — | Not placed |
| Ruined hospitals | **Partly attested** | The hospitals of the final days were improvised cellars (see the first row). Pre-war hospital buildings were not researched. The "Soviet Hospital" road belongs to `a1_horse_road_hospital`. | — | Rank 2 |
| Wire, mines, barricades | **Attested** | Pavlov's House was mined and wired (Gurkin). The Univermag courtyard was mined (Morozov). The slopes of the Long Ravine were "mined and covered in barbed wire" (Aksyonov). Werth saw "barbed wire here, and half-uncovered mines" in February 1943. Sappers laid up to thirty mines a night (Beevor, ch. 10). Residents barricaded streets in late August and September (Beevor, chs. 8–9; Hellbeck, ch. 17). | Wire on low stakes; mines at the edges of rubble; hedgehogs | Ranks 1, 7 and 11; L's dressing |
| Loopholes | **Attested** | Pavlov's House; the L-shaped building; the Mamayev water tanks, where "they'd cut a lot of embrasures" in reinforced concrete about a metre thick (Aksyonov). | Ragged slots at plinth or chest height; half-bricked windows | Ranks 1, 7, 8 |
| Volga-bank dugouts | **Attested; not placeable here** | Strunnikov's photographs [04-21] and [04-22]. The bank looked like "an early frontier mining settlement" (Beevor, ch. 18). The 13th Guards' surgery worked in a drainage pipe (Chuikov, through Hellbeck, "Nurse Vera Gurova"). | Dugout doors; tarpaulin roofs; spill down the slope | Not in the intro: the district has no river bank |

## 4. The tableaux, in ranked order

Reference photographs are cited as `[FF-NN]`, P's identifiers: folder number, then position within
the folder in `manifest.json` order. Paths are relative to
`/home/alex/panzer/reference/wartime_photos_20260928/`. The approach beats are at tank scale; the
beats inside are on foot.

---

### 4.1 `a1_cellar_embrasure_two_layers`: the low window

**Provenance:**

- **Class:** attested_condition.
- **Events:** September–October 1942 (Soviet anti-tank guns in cellars against German armour) and
  20–31 January 1943 (German machine guns in basement embrasures).
- **Depicted:** late January 1943.
- **Place envelope:** any shelled brick building in the city centre.

**Sources**

- **Max Plakolb,** Luftwaffe liaison officer with the 24th Panzer Division, southern end of the city,
  September 1942 (ÖStA-KA B/1540):
  - Quoted in Beevor, ch. 10 "Rattenkrieg": "anti-tank guns concealed in the cellars make it very
    hard going for our advancing tanks." <https://erenow.org/ww/stalingrad/11.php>
  - Source note for p. 149: <https://erenow.org/ww/stalingrad/30.php>
- **Pavlov's House garrison:**
  - ru.wikipedia, citing Gurkin 1963: "narrow embrasures were made in windows and walls", approaches
    mined, wire around the house.
    <https://ru.wikipedia.org/wiki/%D0%94%D0%BE%D0%BC_%D0%9F%D0%B0%D0%B2%D0%BB%D0%BE%D0%B2%D0%B0>
  - Beevor, ch. 12: holes cut for machine guns and anti-tank rifles. Whenever panzers came close, the
    defenders fought from the cellar or the top floor, because "the panzer crews could not elevate
    their main armament sufficiently." <https://erenow.org/ww/stalingrad/13.php>
- **38th Motor Rifle Brigade interviews, 1943** (Hellbeck, "The Capture of Field Marshal Paulus"),
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php>:
  - Captain L. P. Morozov, on the German-held L-shaped building, 29–30 January 1943: "They'd placed
    heavy machine guns in the basements, cut embrasures in the walls. You couldn't see any of it from
    outside."
  - Captain I. Z. Bukharov: the Germans placed their machine guns and snipers, and "the rest would
    stay in the basement".
- **An officer of the 14th Panzer Division,** October 1942 (Beevor, ch. 12): tanks "fired at
  point-blank range in narrow yards. Many of the tanks shook or exploded from the force of an
  exploding enemy mine."

**Attested facts.** Both sides turned semi-basement windows and wall breaches into gun ports. In
September Soviet anti-tank guns in cellars stopped German tanks in the southern city. In the last
days German machine guns fired from basement embrasures that could not be seen from the street. At
Pavlov's House the defenders met approaching tanks from the cellar and the top floor, and the crews
"could not elevate their main armament sufficiently to fire back" (Beevor, ch. 12). The source speaks
only of elevation onto the top floor; nothing here claims a limit on firing down into cellars.

**What the player sees and hears**

- **Approach (30–80 m):**
  - A corner merchant house with a rusticated plinth. Two semi-basement windows at pavement level are
    broken out into low horizontal slots: sills gone, brick splayed, strike pocks concentrated at
    ground-floor height. Snow has drifted into the slots.
  - On the slots' line of fire, 20–40 m out and clear of the driving lane, stands a German medium
    tank from the September advance. It is capped with snow, one track run off the wheels, a
    penetration in the side facing the house. (Autumn layer.)
- **Threshold:** barbed wire on knee-high stakes along the pavement, cut in one place. A trodden
  path runs from the yard to a cellar door.
- **Focus (the cellar):**
  - The slots are splayed inside. One has been narrowed again with loose brick and a sandbag to a
    firing slit. (Winter layer.)
  - A greatcoat is folded on a brick bench. Spent 7.92 mm cases are frozen into the floor ice, beside
    an ammunition box with a blank lid.
  - In the corner lies the September gun, a 45 mm, its shield holed and a wheel gone, shoved aside.
    (Autumn layer.)
- **Residue:** cases in the snow below the slot outside.
- **Sound:** wind pitched by the slot; cases chinking underfoot; a loose fender plate on the wreck
  knocking in gusts.

**Typology and room.**

- **Host:** merchant_brick_house (condition `shelled`, `cellar: true`).
- **Rooms:** cellar (street side) and street (+Z frontage).
- **Alternative host:** apartment_block with a semi-basement.

**Placement constraints**

- The plinth windows are on the street facade, with the sill no more than 0.6 m above street grade.
- The slot axis has at least 40 m of straight sightline.
- The wreck sits on that axis but at least 6 m clear of the Tiger's lane.
- Place it within about 300 m of spawn.
- Keep it away from the live 45 mm sites at (1555, −80) and (1930, −95), or make the static gun
  unmistakably wrecked.
- One per district.

**Must not be shown**

- A gun that reads as manned or live.
- Bodies in the cellar: this source does not attest them.
- Legible markings.
- A replica of Pavlov's House (the four-entrance block), or any claim that this is a named building.

**Teaches.** The danger to a tank is at street level and below. A house front is a fortress face.
The street was fought over twice.

**Reference photographs**

- [08-13] `08_damage_and_recovery/08_destroyed-soviet-tank-snow-ruins-jan-1943_nac-2-942.jpg`:
  rusticated merchant plinth, shop window, pocks concentrated at ground level, snow-capped wreck.
- [07-13] `07_people/07_german-soldiers-in-ruins-winter-42-43_1942.jpg`: brick-vaulted cellar
  opening, timber lintel, German winter smocks.
- [07-07] `07_people/07_snipers-white-suits-entering-ruin_rian-450.jpg`: breaches and dense pocking
  in brick.
- [03-03] `03_soviet_anti_tank/03_ptrd-team-in-stalingrad-ruins_1942-11.jpg`: brick breastwork,
  rubble lumps.
- [03-01] `03_soviet_anti_tank/03_45mm-53k-crew-direct-fire-mozdok_1942-10.jpg`: silhouette of the
  45 mm gun.
- [08-12] `08_damage_and_recovery/08_knocked-out-t34-stalingrad-street_barch-183-b22359.jpg`: a wreck
  in a street.
- [07-12] `07_people/07_german-mg34-team-firing-from-window_barch-101i-617-2571-04.jpg`: a German
  MG 34 team at a window, 23 September 1942.

**Inferred (label in metadata):** the two layers in one cellar; the tank's type; the narrowing with
brick and sandbag; the wreck's position.

**Extends:** ENVSTORY's "Gerhardt's Mill / Pavlov's House" (its practices, not the landmark).

---

### 4.2 `a1_red_cross_cellar`: the red cross over the cellar stair

**Provenance:**

- **Class:** district_event.
- **Place envelope:** the Stalingrad city centre (southern pocket), 20–31 January 1943.
- **Mobility:** within the city centre only. The factory-district counterpart is the catalog's
  `a1_factory_cellar_ward` (December 1942); do not merge the two.

**Sources**

- **Paulus, radio report of 24 January 1943:** "18,000 wounded without the slightest aid of bandages
  and medicines". Adam and Ruhle 2015, p. 193, through
  <https://en.wikipedia.org/wiki/Battle_of_Stalingrad>.
- **Beevor, ch. 22,** <https://erenow.org/ww/stalingrad/23.php>; source notes at
  <https://erenow.org/ww/stalingrad/31.php>:
  - About 20,000 wounded "packed into cellars under the ruins of the city". The theatre cellars held
    600, "with no light and no sanitation" (Dr Hubert Haidinger, 60th Motorized Infantry Division,
    retrospective, 1992).
  - Under the NKVD building: "A red-cross flag outside the entrance"; doctors operating "in the light
    of a field-hospital gas-lamp" (Hans Dibold, *Arzt in Stalingrad*, 1949, pp. 24–25).
  - At the Tsaritsa tunnels: "piles of frozen corpses outside"; "improvised oil lamps as the only
    source of light" (Dr Hermann Achleitner).
- **Beevor, ch. 21,** <https://erenow.org/ww/stalingrad/22.php>:
  - "The shortage of plaster of paris meant that doctors had to bind shattered limbs with paper"
    (Dr Günther Diez).
  - Those who could not be moved "were left behind in the care of a doctor and at least one orderly,
    the standard practice in a retreat."
- **Beevor, ch. 20,** <https://erenow.org/ww/stalingrad/21.php>: frozen corpses left by the field
  hospital "because the ground was frozen too hard to bury them" (Morzik).
- **Hellbeck, "The Capture of Field Marshal Paulus,"** 38th Motor Rifle Brigade, 28–30 January 1943,
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php>:
  - Morozov: in the candy-factory basement, "There was a German hospital there."
  - Major General Burmakov: "That evening we captured a German hospital."
  - Major Yegorov: the Univermag basement "was their hospital. There were about fifty men there:
    wounded, sick, frostbitten."
  - Junior Sergeant Duka: a basement lit by "batteries and headlights from cars"; men "tying blankets
    together".
- **Captain Aksyonov, 24 February 1943** (Hellbeck, "The History Instructor"): in the City Theater
  hospital "They were all mixed together: the sick, the wounded, and the dead."
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/28.php>

**Attested facts**

- In the last fortnight the cellars of the ruined centre became German dressing stations and
  hospitals, marked by a red-cross flag.
- They were lit by improvised oil lamps, a gas lamp or car headlamps on batteries.
- They had no bandages or drugs; paper replaced plaster.
- The dead were piled outside because the frozen ground could not be dug.
- The wounded who could not walk were left behind with an orderly.

**What the player sees and hears**

- **Approach (20–60 m):**
  - A cellar mouth at ground level under a brick segmental arch, with a stair going down.
  - A red-cross cloth, white with a red cross, hangs from a pole beside the arch, frozen stiff.
  - Sledge runners and drag marks converge on it in the snow.
  - Along the wall outside lies a short row of the dead, side by side under tent-halves and blankets,
    drifted with snow.
- **Threshold:** a blanket hung across the stair head; ice on the steps. A tin-can flue pushed
  through a basement window leaks thin smoke.
- **Focus (the cellar):**
  - A low brick vault, the floor covered with straw and blankets in rows along the walls.
  - Rolled paper bandages, mess tins, a bucket.
  - A table made of a door on trestles, with a hissing pressure lamp on it.
  - A few tins with wicks.
  - A pair of vehicle headlamps wired to a battery throwing two hard cones.

  Each is a real emitter, with darkness between them.
- **Residue:** at the far wall a few figures remain under blankets, still. Whether they are alive is
  unresolved.
- **Sound (only near the stair):** coughing and breathing, never words; the pressure lamp's hiss;
  dust trickling from the vault when a distant shell lands; the cloth outside flapping.

**Typology and room.**

- **Host:** merchant_brick_house or apartment_block (condition `shelled` or `partly_collapsed`, with
  the cellar vault intact).
- **Rooms:** cellar (focus); stair or stairwell (threshold); yard or street (approach, and the
  dead).

**Placement constraints**

- `cellar: true`, with the entrance reachable from the street or yard.
- A clear cellar room of at least 8 × 5 m, ceiling at least 2.1 m.
- The approach visible from a street the Tiger uses.
- The row of the dead against a wall, outside vehicle lanes.
- Not within 100 m of rank 6.
- **Outdoor version, if F2 cannot open cellars:** build the approach, the red cross, the dead, the
  blanketed stair head and the lamp glow at its foot. Never paint an interior.

**Must not be shown**

- Surgery, amputations or blood.
- Lice.
- Faces or names.
- A count.
- A swastika flag.
- Legible labels.
- A chaplain melodrama.

Attested but never staged: wounded who could not walk were shot in cellars after the surrender
(Beevor, ch. 22; Senior Lieutenant Fyodorov in Hellbeck). The scene shows the moment before.

**Teaches.** The army that burned this city is dying underground. The red cross is its only sign. The
dead lie above ground because the earth is frozen.

**Reference photographs**

- [07-13] `07_people/07_german-soldiers-in-ruins-winter-42-43_1942.jpg`: cellar arch, timber
  lintel, planks.
- [04-20] `04_stalingrad_city/04_cellar-command-post-red-october_1942-12.jpg`: low vault, a lamp made
  from a shell case, table, telephone cable.
- [07-11] `07_people/07_13th-guards-soldiers-resting-cellar_1942-12.jpg`: a lived-in cellar with
  sacks, an iron bedstead, a samovar.
- [04-11] `04_stalingrad_city/04_people-at-fire-among-ruins-jan-1943_1943-01-31.jpg`: ground in the
  lee of a block, a lone boot, 31 January 1943.
- [06-13] `06_winter_conditions/06_improvised-straw-winter-boots_rian-604297.jpg`: straw overboots.
- [07-04] `07_people/07_german-soldiers-taken-prisoner_1943-02.jpg`: hoods and blankets.
- [09-13] `09_equipment_details/09_erkennungsmarke-id-tag-handbook-page_1943.jpg`: identity tag.

**Inferred (label in metadata):**

- the building type;
- the door-and-trestle table;
- the tin flue;
- the pole for the flag;
- the ambiguous figures;
- the number of the dead.

**Extends:** `a1_factory_cellar_ward` (a different district and date); ENVSTORY's "Univermag
Basement" (without the landmark).

---

### 4.3 `a1_chimney_field_dugout`: smoke from the ground

**Provenance:**

- **Class:** attested_condition.
- **Events:** the timber quarters burned between 23 August and September 1942; civilians lived in
  the ground from autumn 1942 to February 1943.
- **Depicted:** January 1943.
- **Place envelope:** the timber fringes of the city centre.

**Sources**

- **Hellbeck, "The Fate of the City and Its Residents"** (commission interviews),
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/17.php>:
  - The fires could not be fought because the water supply was cut (Denisova, Ioffe).
  - "From August 23 to 28 they'd been staying in shelters, basements, trenches" (Pigalyov).
  - Children were searched for "in courtyards, apartments, trenches, and basements" (Vodolagin).
  - "Some children were found in bunkers."
- **Hellbeck, "Agrafena Pozdnyakova,"** interview of 14 March 1943,
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php>:
  - Courtyard trenches, basements, a sledge.
  - "You'd make something for the children—horse soup."
  - The Germans "left us only hooves and guts".
  - Hellbeck's introduction: 15,000 civilians in German-held areas at the end of November. In
    December soldiers ransacked shacks for food and warm clothing. Units from the steppe sent
    commandos into the city for wood and "demolished whole dwellings".
- **Beevor, ch. 24,** <https://erenow.org/ww/stalingrad/25.php>: a census found "at least 9,796
  civilians", including 994 children. An American aid worker: "Most of the children had been living
  in the ground for four or five winter months."
- **Beevor, ch. 20:** stovepipes "made from empty food tins rammed together".
- **Photographs:**
  - Bundesarchiv Bild 183-B29644, September 1942: residents with their salvaged belongings among
    burned wooden houses.
    <https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-B29644,_Zivilbev%C3%B6lkerung_im_zerst%C3%B6rten_Stalingrad.jpg>
  - NAC 2-939 (Bauer, German PK), October 1942: plank-framed shelter entrances dug into a
    city-centre garden, one of them caved in.
    <https://commons.wikimedia.org/wiki/File:Stalingrad._Zniszczone_schrony_i_ruiny_miasta._(2-939).jpg>

**Attested facts.** The timber quarters burned down to their stoves and chimneys. Through the winter,
civilians lived in trenches, dugouts and cellars among them, heated by small stoves with tin
stovepipes, moving goods on sledges. Soldiers took their fences and houses for firewood and searched
them for food.

**What the player sees and hears**

- **Approach:** a grid of burned plots, one brick stove mass and chimney per plot at plot pitch. Ash
  and charred sill logs lie under snow, with iron bedsteads and pots among them. Fence posts stand
  without their boards. (Autumn fire, then winter stripping.)
- **Focus:** between two stoves, a dugout.
  - A shallow pit roofed with salvaged doors, sheet iron and earth, capped with snow.
  - A short tin-can stovepipe breathes faint smoke from a small fire inside, modelled as a source:
    the only warm point in the field.
  - A narrow entrance hung with a blanket; a small sledge with a sack; a bucket.
- **Inside (optional):** straw and bedding, a small iron stove, a pot, a horse's hoof and lower leg
  on the frozen floor, and a tin lamp.
- **Residue:** a trodden path leads away toward the nearest water. If the catalog's
  `a1_oil_water_rail_siding` is placed nearby, the path points toward it.
- **Sound:** the stovepipe's draught and a faint crackle; wind moaning over the chimney tops.

**Typology and room.**

- **Host:** wooden_house, condition `burned`. The grammar leaves only the stove and chimney; the
  timber is absent from the blueprint.
- **Rooms:** yard (the dugout). Where the grammar gives the house an under-floor pit, the dugout may
  reuse it.

**Placement constraints**

- The timber fringe of the district, in a group of at least three burned plots.
- The dugout in a yard, never in a street.
- The smoke must come from a modelled fire.
- One per district, at least 150 m from rank 9.

**Must not be shown**

- Children's bodies, faces or posed "mother and child" figures.
- Chalked messages on walls: these are attested (Beevor, ch. 24) but are text.
- Icons or papers.
- Soldiers looting.

**Teaches.** People live in the ground among the homes your war burned. A thread of smoke is a life.

**Reference photographs**

- [04-27] `04_stalingrad_city/04_stalingrad-south-ruins-colour-autumn-1942_barch-169-0902.jpg`: rows
  of chimneys; colour.
- [07-14] `07_people/07_residents-returning-sledge-feb-1943_1943-02.jpg`: a sledge with bundles past
  chimney stacks.
- [04-22] `04_stalingrad_city/04_dugouts-cut-into-volga-bank_1943.jpg`: dugout doorways; timber and
  tarpaulin roofs.
- [06-13] `06_winter_conditions/06_improvised-straw-winter-boots_rian-604297.jpg`: dugout openings in
  a slope.
- [09-06] `09_equipment_details/09_dugout-stove-mess-tins_barch-101i-394-1459-29.jpg`: a sheet-metal
  stove and stovepipe.
- The two Commons photographs above (B29644, NAC 2-939).

**Inferred (label in metadata):** the dugout's construction; the hoof; the path's target; the choice
of plot.

**Extends:** `a1_burned_cellar_household` (Pozdnyakova, a September event) is not restaged here. This
is the winter condition in a different plot. It also extends `a1_apartment_firewood`, adding the
German wood commandos as a cause of the stripping.

---

### 4.4 `a1_tram_on_its_rails`: the tram that stopped on 23 August

**Provenance:**

- **Class:** attested_condition. The photograph is the physical reference, not the location.
- **Events:** the service stopped on 23 August 1942; the photograph dates from February 1943.
- **Depicted:** late January 1943.

**Sources**

- **Yefim Kopyt,** "Tulak Street (today Bazisnaya Street). A tram carcass riddled by bullets and
  fragments in the central part of Stalingrad", February 1943 (Stalingrad Battle Museum-Reserve).
  - Commons, where the categories include "Dead horses":
    <https://commons.wikimedia.org/wiki/File:%D0%9D%D0%B0_%D0%BE%D0%B4%D0%BD%D0%BE%D0%B9_%D0%B8%D0%B7_%D1%83%D0%BB%D0%B8%D1%86_%D0%A1%D1%82%D0%B0%D0%BB%D0%B8%D0%BD%D0%B3%D1%80%D0%B0%D0%B4%D0%B0.jpg>
  - Exhibition report: <https://v1.ru/text/culture/2017/12/22/53300861/>
- **The Stalingrad tram:**
  - It ran until 23 August 1942 and was destroyed completely during the battle. Kh-series cars were
    used from 1929; the gauge is 1,524 mm; the first restored car ran in December 1943.
    <https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%B0%D0%BC%D0%B2%D0%B0%D0%B9>
  - Demchenko (Hellbeck, "The Fate of the City"): after the first bombardment "we decided to repair
    the water system and the bread factory … and even to get the trams running."
- **The Kh motor car**
  (<https://ru.wikipedia.org/wiki/%D0%A5_(%D1%82%D1%80%D0%B0%D0%BC%D0%B2%D0%B0%D0%B9%D0%BD%D1%8B%D0%B9_%D0%B2%D0%B0%D0%B3%D0%BE%D0%BD)>):
  - two axles, 2,700 mm wheelbase;
  - body 9,565 mm long (10,270 mm over the buffers), 2,464 mm wide, 3,274 mm from rail head to roof;
  - 12.65 t empty;
  - riveted steel frame with 3 mm steel panels below the windows; roof, window frames, doors and
    lining of wood; a plank floor.

  A 1930s Kh at the tractor-factory terminus:
  <https://commons.wikimedia.org/wiki/File:Tramway_Stalingrad_STZ.jpg>
- **No trolleybuses:** the system opened on 31 December 1960.
  <https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%BE%D0%BB%D0%BB%D0%B5%D0%B9%D0%B1%D1%83%D1%81>

**Attested facts.** The trams stopped when the city burned. Cars stayed where they stood. By February
1943 a riddled car stood on its rails in a central street, with span wires still up on
double-bracket poles and a frozen dead horse lying in the snow beside the line.

**What the player sees and hears**

- **Approach (tank scale):** a two-axle Kh tram car on its rails, the street's rails flush in the
  snow.
  - Every pane is gone. The wooden roof sheeting is torn and carries snow.
  - The steel panels below the windows are holed by bullets and fragments; a door hangs open.
  - Snow has drifted over the slatted seats inside. The route boards are blank.
- **Overhead:** a pole with two curved bracket arms carries the span wires; one wire has fallen into
  a sagging arc.
- **Focus:** a few metres from the car a dead horse lies on its side, legs out, frozen stiff. The
  snow around it is trampled by people passing.
- **Residue:** a crate and a bucket half-sunk in the snow nearby; a picket fence across the street.
  (As in the photograph.)
- **Layers:** autumn (the stopped car, pocks, burned frontages); winter (the horse, snow, the
  trampled path).
- **Physics:** the car is a 12.65 t body on a steel underframe. The 57 t Tiger can shove it along the
  rails or off them, and its wooden roof can splinter. Its collision proxy comes from its mesh.
- **Sound:** wind humming and whistling in the span wires; a loose window frame knocking; snow
  squeaking underfoot.

**Typology and room.** Street: a tram street in front of merchant or wooden frontages. Attach it to a
building's street-frontage anchor.

**Placement constraints**

- L's tram line must exist in that street; the car sits on it.
- Leave a lane of at least 6 m clear, so that pushing the car is the player's choice.
- Keep the horse off the Tiger's line.
- Place it early on the route, as the establishing image near spawn.

**Must not be shown**

- Trolleybuses, or twin trolley wires.
- Readable route numbers.
- Bodies inside the car: not attested.
- Butchering of the horse: the photograph shows it whole, and butchering belongs to the catalog's
  horse scenes.
- Slogan boards.

**Teaches.** This was a working city with trams and daily routes. Its life stopped on one afternoon.
Months later, even the horses lie where they fell.

**Reference photographs**

- The Kopyt photograph (Commons, above).
- 'Tramway Stalingrad STZ' (Commons): the form of a Kh car.
- [08-12] `08_damage_and_recovery/08_knocked-out-t34-stalingrad-street_barch-183-b22359.jpg`: tram
  rails flush in an earth street, October 1942.
- [08-13] (see §4.1): a fallen pole across a square; lamp posts.
- [04-23] `04_stalingrad_city/04_former-sadovaya-street-signpost_1943.jpg`: a vanished street marked
  by a pole.
- [03-04] `03_soviet_anti_tank/03_76mm-usv-crew-firing-on-stalingrad-street_1942-10.jpg`: a timber
  street with fences and a pole.

**Inferred (label in metadata):**

- the tram's position and orientation;
- the fallen wire;
- the knocking frame;
- that this car stopped on 23 August. The photograph does not date its stopping; the network did.

**Extends:** it adds nothing to the catalog's horse scenes beyond the photographed carcass.

---

### 4.5 `a1_white_canopy_canister`: white canopies

**Provenance:**

- **Class:** attested_condition.
- **Window:** both pockets, 23 January – 2 February 1943.
- **Depicted:** late January 1943.

**Sources**

- **Beevor, ch. 22,** <https://erenow.org/ww/stalingrad/23.php>:
  - "Resupply now depended on canisters dropped by parachute, 'the supply bombs', but despite Sixth
    Army's requests for red canopies, the Luftwaffe continued to use white" (Winrich Behr, conversation
    with Beevor, 1995).
  - "few units had any recognition panels left".
  - General Hube's order: on hearing aero-engines, lie down on the snow in the form of a cross to mean
    "German soldiers here" (Army Group Don records, BA-MA RH19 VI/12, p. 451).
  - German signal flares were answered by Soviet flares of the same colour.
  - "Some men were so desperate that they risked trying to retrieve canisters right out in the open.
    Russian snipers picked them off with ease."
- **Beevor, ch. 21:** "Men dreamed of coming across a parachute container … but they had been either
  seized on landing, or lost in the snowfields."
- **Beevor, ch. 20:** "Perhaps some companies retrieved odd canisters and kept them" (Behr to Hitler).
- **Sergeant Helmut Pist,** 24th Panzer Division, interrogated on 9 February 1943: "The air-dropped
  food supplies were being taken by whoever found them, despite strict orders and the threat of being
  shot." (Hellbeck, "German Prisoners in February 1943").
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/32.php>
- **Adam and Ruhle** (through <https://en.wikipedia.org/wiki/Battle_of_Stalingrad>): after 23
  January, no landings, only intermittent drops of ammunition and food.

**Attested facts.** At the end the pocket was supplied only by white parachute canisters. The wind
blew them across the lines. Men were shot trying to reach canisters in the open. What was found was
kept by whoever found it.

**What the player sees and hears**

- **Approach (from a distance):** high on a gutted apartment block, at the fourth or fifth storey, a
  white canopy is snagged on a broken joist or on roof iron. Its shroud lines trail down the facade.
  - Against grey sky it reads as a pale shape.
  - White on snow, it is low-contrast and honest to the light.
- **Focus:**
  - In the open, in a square or yard 20–60 m from the nearest cover, a dark cylindrical canister lies
    half-sunk in snow, unopened.
  - A second canister near a cellar mouth is split and empty, its lid off, with tool marks.
- **Residue:** a single line of footprints and drag marks runs from the cellar mouth toward the
  unopened canister. It ends halfway, at a low mound under snow.
- **Sound:**
  - The canopy snapping in gusts; shroud lines ticking against the wall.
  - At night, distant transport engines overhead and paired flares of one colour, if the district's
    ambience carries them.

**Typology and room.**

- **Canopy host:** apartment_block (condition `burned` or `shelled`).
- **Canister:** yard or square.

**Placement constraints**

- The unopened canister has at least 20 m of open ground to the nearest cover. The openness is the
  danger.
- The canopy hangs on the district's tallest ruin, visible from spawn.
- Stencils are illegible marks.
- If the canopy animates, the district's wind state drives it. Otherwise it is a static draped mesh,
  named as static in code.
- One per district.

**Must not be shown**

- Absurd cargo: that is ENVSTORY's steppe scene "The Absurd Supply Drop Field".
- Readable stencils.
- The shot itself.
- A posed body.
- Soldiers lying in the cross formation (living people).
- Germans shooting Germans over food: attested only as a threat.

**Teaches.** The lifeline from the air is failing. The open ground between ruins kills. What falls
from the sky is not for you.

**Reference photographs**

- [05-07] `05_steppe_and_pocket/05_air-supply-drop-containers_1943.jpg`: canisters about 2.5–2.7 m
  long (P's estimate), with stencilled handling text.
- [04-09] `04_stalingrad_city/04_gutted-apartment-block-through-arch_1943.jpg`: exposed floors and
  joists, where a canopy could snag.
- [04-01] `04_stalingrad_city/04_city-centre-after-liberation-zelma_rian-602161.jpg`: an open snow
  plain between wall stacks, 2 February 1943.
- [06-12] `06_winter_conditions/06_soldiers-around-fire-short-lull_rian-137429.jpg`: open trodden
  ground between ruins, 7 January 1943.
- [04-11]: debris on snow.

**Inferred (label in metadata):**

- the canister's type (the photograph's place is not named);
- two canisters rather than a field of them;
- the mound;
- the tool marks.

**Extends:** ENVSTORY's "The Absurd Supply Drop Field" (a steppe scene about absurd cargo). This city
scene is about denial and exposure instead.

---

### 4.6 `a1_basement_with_stovepipe`: the basement with the stovepipe

**Provenance:**

- **Class:** district_event: the southern city centre, 28–29 January 1943.
- **Supporting instance:** the 297th Infantry Division, southern part of the pocket, January 1943.
  Its exact place is not given.

**Sources**

- **Junior Sergeant Alexander S. Duka,** mortar man, 2nd Battalion, 38th Motor Rifle Brigade;
  commission interview 1943 (Hellbeck, "The Capture of Field Marshal Paulus").
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php>
  - "I noticed smoke coming from a chimney and realized they must be in there."
  - "then this old Russian man came out from the basement. He said there were civilians down there.
    There turned out to be eleven Germans, five wounded."
  - "They'd brought their wounded to this basement. The woman living there helped them: she cleaned
    their wounds, bandaged them."
- **Gebele,** 297th Infantry Division, retrospective, through Beevor, ch. 22:
  - "Two Stalingrad women rubbed my frozen legs for an hour to prevent the effects of severe
    frostbite."
  - Women in a partly wrecked house "had just baked some bread, and agreed to exchange a loaf for a
    hunk of frozen horsemeat."
- **Pozdnyakova** (Hellbeck, see §4.3): flat bread from spoiled flour; soldiers who came in to get
  warm and then took things.

**Attested facts.** In the last days civilians and German wounded shared domestic cellars. A woman
cleaned and bandaged enemy soldiers' wounds. Chimney smoke gave the occupied cellar away. Bread was
traded for horsemeat.

**What the player sees and hears**

- **Approach:** a two-storey house whose upper floor burned in the autumn. A tin stovepipe is pushed
  out of a semi-basement window, and smoke rises from a modelled fire inside. A cellar door is hung
  with a blanket; a narrow trodden path leads to it; a small sledge stands by.
- **Focus (one cellar, two corners):**
  - **The household's corner:** bedding on boards, a trunk, pots, a flat loaf on a board, a bucket of
    water with a skin of ice, a small iron stove.
  - **The soldiers' corner:** greatcoats spread as bedding, two helmets, a rifle against the wall,
    rags in a basin, a pan of water on the stove, a lump of frozen horsemeat on a board.
- **Residue:** nobody is there. Whether they were captured or moved on is unresolved.
- **Sound:** the stove ticking and crackling; wind across the pipe mouth.

**Typology and room.**

- **Host:** merchant_brick_house (small, two storeys) or a wooden_house on a brick plinth with a
  cellar.
- **Rooms:** cellar (domestic) and yard.

**Placement constraints**

- `cellar: true`, 5–8 m × 4–6 m.
- The stovepipe on a yard or street facade.
- At least 100 m from rank 2, which is the military counterpart.

**Must not be shown**

- The shooting on the basement stairs (in Duka's account a German killed two Soviet soldiers there).
- The grenade proposed for the chimney.
- Any sexual violence: Hellbeck's introduction attests rapes by German soldiers, and they are never
  shown.
- Faces; a named family; icons or legible papers.

**Teaches.** In the cellars the invaders' wounded and the city's people shared one fire. Care crossed
the front line. Smoke gives life away.

**Reference photographs**

- [07-11]: cellar life.
- [04-20]: cellar light and a table.
- [09-06]: a stove and stovepipe.
- [07-13]: a cellar opening.
- [04-11]: people at a fire in the lee of a block, 31 January 1943.

**Inferred (label in metadata):**

- the house type;
- the division of the cellar into two corners;
- the objects in each corner (the sources name a stove chimney, the wounded, bandaging, bread and
  horsemeat);
- the absence of the people.

**Extends:** it is new; it complements rank 2.

---

### 4.7 `a1_floor_breach_strongpoint`: through the floor

**Provenance:**

- **Class:** attested_condition.
- **Instances:** the L-shaped house on 9 January Square, 3 December 1942; the L-shaped building in the
  southern centre, 29–30 January 1943; the clearing of cellars in the centre on 31 January 1943.
- **Depicted:** late January 1943.

**Sources**

- **Major General Alexander Rodimtsev,** 13th Guards Rifle Division, commission interview (Hellbeck,
  "Guards Division General Alexander Rodimtsev").
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/24.php>
  - Saps were dug over eight days to within 20–30 m of the German-held L-shaped building, the railway
    workers' building and School No. 38. "They didn't toss the dirt out of the trench but carried it
    down to the Volga."
  - "We took control of the top floors … they were down below."
  - "In one basement we broke through the ceiling with crowbars and had at them with three
    flamethrowers … In another cellar they put 250 kilograms of TNT on the floor above and detonated
    it."
  - "we had to take out the bodies, both ours and theirs."
- **Morozov** (see §4.1): German basement machine guns and embrasures, a night assault, the building
  surrounded on three sides.
- **Beevor, ch. 22:** by 31 January "Ruined buildings and cellars had been cleared with grenade and
  flame-thrower."
- **English Wikipedia,** citing Adam and Ruhle and Beevor pp. 135–137: "Germans and Soviets on
  alternate levels, firing at each other through holes in the floors."
- **ru.wikipedia, Pavlov's House** (Adam, citing Colonel Roske): German assault groups took the lower
  floor but could not take the whole building.

**Attested facts.** Buildings were fought for storey by storey. Garrisons in basements were attacked
through the floor above with crowbars, flamethrowers and charges. Saps were dug to within 20–30 m.
Wire was cut and embrasures were blinded.

**What the player sees and hears**

- **Approach:**
  - A four- or five-storey 1930s apartment block with balconies, shelled.
  - Across the street or yard, a narrow zig-zag sap runs to within 20–30 m of it. It has no spoil
    heaps, because the spoil was carried away.
  - Cut wire lies at the block's base.
  - Soot fans spread upward from the basement embrasures, where smoke vented out.
- **Focus (stairwell and ground-floor dwelling):**
  - A ragged hole about 0.6–1 m across is broken through the floor into the cellar below: brick
    vault or timber joists with splintered ends. Soot fans up the walls around it.
  - In another room the floor has dropped into the cellar, with joists hanging. This is a failed slab
    module in the structural state, not a paint-over.
- **Residue (cellar, if reachable):**
  - Walls and vault black with soot.
  - Blackened helmets, mess tins and a stove.
  - The embrasures still look out at the street.
  - No bodies: they were carried out, and the absence carries the scene.
- **Sound:** a draught through the floor hole; joists creaking; ice ticking.

**Typology and room.**

- **Host:** apartment_block (condition `shelled` or `partly_collapsed`, `cellar: true`).
- **Rooms:** stairwell, ground-floor dwelling, cellar; yard or street (the sap).

**Placement constraints**

- The block faces an open field of 30–50 m, which the sap crosses at foot scale, 0.6–0.8 m wide.
- If the sap crosses the Tiger's route, it is real terrain that tracks physically cross.
- The breach sits above a real cellar room in the blueprint. The failed slab comes from
  `condition_damage`, or from `BuildingState::apply_damage` at instantiation.

**Must not be shown**

- Burned bodies.
- Flamethrower crews.
- Executions.
- A red flag with a slogan.
- Figures.
- Any name: this is neither the L-shaped house nor the railway workers' house.

**Teaches.** Floors are not safe. The enemy comes from above and below. A fortress becomes a tomb.

**Reference photographs**

- [04-09]: a gutted block with an exposed stair tower and basement windows at grade.
- [04-11] `04_stalingrad_city/04_people-at-fire-among-ruins-jan-1943_1943-01-31.jpg`: a 1930s block
  with balconies, a breach at ground level, soot above it.
- [07-07]: breaches in brick walls.
- [04-02] `04_stalingrad_city/04_snow-field-of-ruins-from-shattered-wall_rian-2251.jpg`: a view
  through a shell-holed wall.
- [04-12] `04_stalingrad_city/04_fallen-fighters-square-standing-ruin_1943-02.jpg`: a lone wall.
- [04-04] `04_stalingrad_city/04_soldiers-through-ruined-street-canyon_rian-2391.jpg`: a street
  canyon of gutted blocks.

**Inferred (label in metadata):**

- the hole's size;
- the charred objects;
- the sap's route;
- combining two methods (crowbar breach and charge) in one building.

**Extends:** it is new. The exact landmarks stay out.

---

### 4.8 `a1_kitchen_firing_post`: the kitchen chair at the shell hole

**Provenance:**

- **Class:** attested_condition.
- **Events:** September 1942 – January 1943, used by both sides.
- **Depicted:** January 1943.

**Sources**

- **Vasily Grossman,** notes (RGALI 618/2/108), through Beevor, ch. 10
  (<https://erenow.org/ww/stalingrad/11.php>; source note p. 148):
  - Fighting in "brick-strewn, half-demolished rooms and corridors" of apartment blocks, "where there
    might still be a vase of withered flowers, or a boy's homework open on the table".
  - An artillery spotter "through a convenient shell-hole in the wall, seated on a kitchen chair".
  - Beevor: the detritus of war ("shell cases, signal wire and grenade boxes") mixed with "iron
    bedsteads, lamps and household utensils".
- **Bundesarchiv Bild 101I-617-2571-04** (Ollig, 23 September 1942), [07-12]: a German MG 34 team
  firing from the window of a flat over the rooftops.
- **Pavlov's House:** an artillery observation post on the top floor (ru.wikipedia).
- **The 38th Motor Rifle Brigade interviews:** the Germans kept snipers on the roofs (Karpov).
- **Bundesarchiv 183-E0406-0022-001,** January 1943, [07-08]: riflemen behind a roof parapet.

**Attested facts.** Upper-floor flats were observation and firing posts for both sides. The
household's objects stayed among the detritus of war.

**What the player sees and hears**

- **From the street:** on the third or fourth storey, a shell hole in the outer wall of a flat.
  Through it, the back of a kitchen chair and the edge of a table show against the dark room.
- **Focus (on foot, up the stairwell):**
  - The chair faces the hole. A binocular rest stands on the table.
  - Spent German 7.92 mm cases lie beneath the hole. (Autumn layer.)
  - A cut telephone cable leaves the flat and runs down the stairwell.
  - The household: an iron bedstead, an oil lamp, a sideboard with broken crockery, a vase of dead
    flowers frozen in place, a closed exercise book with a blank cover.
- **Winter layer:** snow blown in through the hole lies in a fan across the chair and floor.
- **Sound:** wind through the hole; a loose sash knocking; plaster flakes falling.

**Typology and room.**

- **Host:** apartment_block, or a merchant house with an upper-floor flat (condition `shelled`).
- **Rooms:** upper-floor dwelling and stairwell.

**Placement constraints**

- The hole faces a long sightline (a street or square of at least 80 m).
- If upper floors are not walkable, place the chair and table so they read through the hole from the
  street.

**Must not be shown**

- Readable homework or letters.
- Photographs of identifiable people.
- A body at the chair: not attested.

**Teaches.** The war moved into kitchens. Every window was an eye. Domestic life stopped mid-gesture.

**Reference photographs**

- [07-12] `07_people/07_german-mg34-team-firing-from-window_barch-101i-617-2571-04.jpg`
- [07-08] `07_people/07_red-army-men-firing-from-rooftop-jan-1943_barch-183-e0406-0022-001.jpg`
- [04-02]: the view through a shell-holed wall over a snowfield of ruins, 23 December 1942. This is
  the view from the post.
- [04-09], [04-11]

**Inferred (label in metadata):** the specific furniture; the binocular rest; the calibre of the
cases; the storey.

**Extends:** it is new.

---

### 4.9 `a1_evicted_household_yard`: turned out into the yard

**Provenance:**

- **Class:** attested_condition: German-held Stalingrad, December 1942 – January 1943.
- **Instance:** Pozdnyakova, 26 January 1943.

**Sources**

- **Pozdnyakova** (Hellbeck, "Agrafena Pozdnyakova"),
  <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php>:
  - "On the 26th, when our forces were about to enter the city, the Germans occupied our building,
    which became their headquarters. They kicked us out into the courtyard at four in the morning,
    with all the kids and our things. We stayed in the trenches for two days until our soldiers
    arrived."
- **Hellbeck's introduction to the chapter,** drawing on Soviet documents: "As the 6th Army fell
  back, officers and soldiers took up lodging in undamaged residences and threw the inhabitants out
  into the street." It also describes December's ransacking for food and warm clothing.
- **Photographs:** Bundesarchiv 183-B29644 (belongings in the open) and NAC 2-939 (yard shelter
  mouths), both in §4.3.

**Attested facts.** In the last weeks German soldiers took the few roofed houses and put the families
out into their yard trenches, with whatever they could carry, in deep frost.

**What the player sees and hears**

- **Approach:** one of the few roofed houses, showing signs of occupation.
  - A field-telephone cable is strung in through a window.
  - The windows are blanketed from inside.
  - A trampled patch by the door where a sentry stood.
  - Ration boxes with blank lids stacked at the step.
- **Focus (the yard):** a slit trench from the summer's air raids, roofed with a door and boards.
  In and beside it lie the household's things: bundles tied in blankets, a trunk, a pot, rolled
  bedding, a child's sledge. Everything is rimed with frost, with fresh snow on the bundles.
- **Sound:**
  - From the house: a stove's draught, and occasionally a field-telephone bell, which carries no
    words.
  - From the yard: nothing.

**Typology and room.**

- **Host:** wooden_house or merchant_brick_house (condition `intact`, or lightly `shelled` with the
  roof on).
- **Rooms:** yard (the trench) and ground-floor dwelling (occupied).

**Placement constraints**

- The yard is visible from the street through a gate or a gap in the fence.
- The house is one of the rare roofed ones.
- At least 150 m from rank 3.

**Must not be shown**

- Violence, or an eviction in progress.
- Faces.
- The family's fate: unknown; Pozdnyakova's family survived.
- Unit signs.
- A swastika.

**Teaches.** Your army, freezing, took the last warm rooms and put families into the snow.

**Reference photographs**

- [03-04]: a timber street with fences.
- [06-05] `06_winter_conditions/06_radio-halftrack-in-snow-village-jan-1943_barch-101i-235-0962-05a.jpg`:
  low houses in snow, January 1943.
- [07-14]: bundles on a sledge.
- The two Commons photographs (B29644, NAC 2-939).

**Inferred (label in metadata):** the occupation signs; the trench's roof; the objects; the telephone
bell.

**Extends:** `a1_burned_cellar_household` (Pozdnyakova's September event is separate and not
restaged); `a1_apartment_firewood`.

---

### 4.10 `a1_yard_of_the_unburied`: where the ground would not open

**Provenance:**

- **Class:** attested_condition.
- **Window:** mid-January to the end of January 1943.
- **Contrast:** autumn graves, from October 1942.

**Sources**

- **Beevor, ch. 20** (<https://erenow.org/ww/stalingrad/21.php>): at Pitomnik, "the piles of frozen
  corpses, left by the field hospital there because the ground was frozen too hard to bury them"
  (Morzik, p. 188).
- **Beevor, ch. 22:** "piles of frozen corpses outside" the Tsaritsa tunnels in the city
  (Achleitner).
- **Beevor, ch. 21** (<https://erenow.org/ww/stalingrad/22.php>):
  - At Gumrak "The corpses lay in heaps beside the road" (Stolberg report, 17 February 1943, BA-MA
    RW4/v.264, p. 160).
  - Chaplains snapped off the lower half of the identity disc "as official proof of death".
  - "Those in need of more clothes stripped corpses of clothing as soon as they could after the moment
    of death. Once a body froze, it became impossible to undress."
- **Pist** (Hellbeck, "German Prisoners in February 1943"): "thousands of dead bodies, and the wounded
  men dying in the streets because the hospitals were overcrowded".
- **Beevor, ch. 12:** in October German battalions "sent back the corpses of their comrades at night
  for burial" (the autumn practice).
- **The next state,** outside the intro's window: on 24 February the dead were collected in piles in
  the centre (Aksyonov). Civilian burial parties stacked them "like piles of timber" and moved them
  on sledges and handcarts (Beevor, ch. 24).

**Attested facts.** In autumn the dead were buried with markers. By January the frozen ground and
exhaustion left them in piles by aid posts and roads. Their tags were broken, and their clothes and
boots were taken before they froze.

**What the player sees and hears**

- **Approach:**
  - In a block's yard, the dead lie in a line along the wall under blankets and tent-halves, drifted
    with snow. On some, the boots are gone.
  - A pick and a shovel lie beside a shallow scrape in the frozen ground, abandoned.
- **Autumn layer:** at the gate, two or three older grave mounds, each with a birch cross and a helmet.
- **Detail:** a small tin by the wall holding broken tag halves, their stampings illegible.
- **Sound:** silence and wind; a frozen tarpaulin creaking.

**Typology and room.**

- **Host:** apartment_block or merchant_brick_house.
- **Room:** yard.

**Placement constraints**

- Best as the yard of rank 2. Otherwise near a former aid post.
- Never on a street the Tiger must drive.
- Keep the number low: it is an authoring choice, not a count.

**Must not be shown**

- Gore or faces.
- A total.
- Mass graves.
- Bodies being thrown.
- Burial parties: these come after the surrender.
- Cannibalism: never. Beevor reports it in prison camps; it has no place here.

**Teaches.** The earth refused. The dead outnumber the diggers. The neat graves of autumn are over.

**Reference photographs**

- [05-12] `05_steppe_and_pocket/05_german-war-graves-gorodishche_1943.jpg`: grave mounds marked with
  helmets.
- [09-13]: the two-part identity tag.
- [04-11]: a lone boot on trampled snow.

**Inferred (label in metadata):** the scrape and the tools; the tin of tag halves; the mounds at the
gate; the choice of yard.

**Extends:** `a1_red_october_factory_cemetery` (a factory-belt landmark); this is the city-centre
condition.

---

### 4.11 `a1_residents_barricade`: the street the residents closed

**Provenance:**

- **Class:** attested_condition.
- **Event:** late August to mid-September 1942 (autumn layer only).
- **Depicted:** its remains, January 1943.

**Sources**

- **Beevor, ch. 8,** <https://erenow.org/ww/stalingrad/9.php>:
  - The Military Council's state-of-siege posters: "Let us barricade every street. Let us transform
    each district, each block, each building into an impregnable fortress."
  - The Stalingrad Defence Committee, 26 August 1942: "organize brigades, go to build barricades.
    Barricade every street…" (both from the Central Museum of the Armed Forces, TsMVS).
- **Beevor, ch. 9,** <https://erenow.org/ww/stalingrad/10.php>: Chuikov "described the improvised
  defences he found as little more than barricades which could be pushed over with the front of a
  truck."
- **Hellbeck, "The Fate of the City and Its Residents":**
  - After the state of siege the Red October steelworks made "machine-gun clusters, tank traps, and
    shovels".
  - 14 September: a district committee meeting "in a trench" heard "reports on barricade
    construction" (Denisova).

**Attested facts.** Residents' brigades barricaded the streets in late August and September. The
barricades were flimsy. The steelworks made tank traps.

**What the player sees and hears.** Three to five steel hedgehogs lie across a street: two shoved
aside, one on its back. Behind them is a low bank of paving setts, earth and timber with a
vehicle-wide gap pushed through it. Snow has drifted against the gap's edges. If there is a tram line,
its rails in the gap are bent.

**Typology and room.** Street.

**Placement constraints.** A street across the district, off the main route, or crossed through its
gap.

**Must not be shown**

- Posters or slogans.
- Heroic tableaux.
- A claim about a specific street.

**Teaches.** The city's people tried to close their streets against tanks. The tanks came through.

**Reference photographs:** none in the set shows a Stalingrad barricade. For context only: [08-12]
(tram rails) and [03-04] (a timber street).

**Inferred:** the physical form. The sources attest barricades and tank traps but do not describe
them. **Do not build this scene until a dated photograph of a Stalingrad street barricade is found.**

## 5. Sound and weather for the whole district

These come from the same sources. Use them as ambience driven by physical sources only.

- **Night bombers.** Soviet U-2 biplanes droned toward the German lines, cut their engines, dropped
  their bombs, and restarted their engines on the way back (Aksyonov, Hellbeck, "The History
  Instructor").
- **"The usual melody of the front"** (a lieutenant, through Beevor, ch. 21): "the dull crack of a
  sniper's rifle, the odd burst of machine-gun fire, and the distant whistle at night of a signal
  flare going up".
- **Flares.** German flares for the supply aircraft were answered by Soviet flares of the same colour
  (Beevor, ch. 22). Seen as paired flares at night.
- **Soviet loudspeakers.** They played "the monotonous ticking of a clock" and "deathly tango dance
  music" between broadcasts (Beevor, ch. 18, <https://erenow.org/ww/stalingrad/19.php>). Use only
  the clock and the music, only within earshot of the front, and never the voice.
- **Weather.**
  - A mild, wet spell at the very start of January, then −35 °C on 5 January (the marine Barsov,
    Beevor, ch. 19, <https://erenow.org/ww/stalingrad/20.php>). The result is a thaw crust under fresh
    snow and an ice glaze on the rubble.
  - In mid-January, "the sun on the snow was blinding", with steel-blue evening shadows (Beevor,
    ch. 21).
  - In early February, −25 to −30 °C (Beevor, ch. 24).
  - On 2 February, "thick mist", later dispersed by sun and a wind "which whipped up the powdery snow"
    (Beevor, ch. 22).
  - P gives snow depths of 20–40 cm.

## 6. Corrections and cross-references for existing documents

1. **Trolleybuses.** No trolleybus ran in Stalingrad before 31 December 1960. Remove "trolleybuses"
   from any brief, and never string twin trolley wires.
2. **ENVSTORY, "Univermag Basement".** Several props in that diorama have no source:
   - the map swept onto the floor;
   - a general's greatcoat folded on a cot;
   - "an untouched last meal";
   - field telephones with cut wires;
   - Red Army rifles stacked outside.

   What the sources do support:
   - The basement was a hospital with about fifty wounded, sick and frostbitten men (Yegorov).
   - The corridors served as latrines; "the filth came up to your chest" (Soldatov, with Roske's
     explanation).
   - The courtyard was "mined all around" (Morozov).
   - Negotiations took place in a colonel's office next to Paulus's room (Morozov).
   - A swastika banner hung from the balcony above the main entrance (Beevor, ch. 22).

   All of these are in Hellbeck, "The Capture of Field Marshal Paulus".
3. **Catalog `a1_white_sheet_surrender_road`.** ACT_ONE_HISTORICAL_SCENES.md says its source still
   needs passage-level verification. The passage is in Hellbeck, "The History Instructor: Captain
   Nikolai Aksyonov"
   (<https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/28.php>), at the Long Ravine in
   late January 1943:
   - "we took him to a high place, gave him a sheet, and ordered him to signal his men to surrender
     … Scouts and officers disarmed them and stacked their weapons in a pile."
   - "One of them had a blanket on each foot. Others wore sheets and looked like scarecrows."

   This supports all three of its beats. The sheet was waved by a captured German commander on Soviet
   orders; record that in its metadata.
4. **Catalog `a1_factory_cellar_ward` (KP 25224).** Hellbeck's "A German Diary from the Kessel"
   (<https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/33.php>) prints an English
   translation of a German diary from 22 November to 18 December 1942. Its 15–18 December entries
   match the catalog's source: a factory basement, a fire lit on the floor, a doctor found in another
   basement. Check whether it is the same document before citing it as independent corroboration.
5. **Catalog `a1_horse_road_hospital` and `a1_grain_elevator_extraction_convoy`.** Pozdnyakova's
   English text (Hellbeck, "Agrafena Pozdnyakova") gives passage-level wording for both:
   - "They left us the legs, heads, entrails … only hooves and guts."
   - "a German for every Russian prisoner" moving the grain carts.
6. **Adding to the catalog.** `tools/author_historical_scenes_astra.py:233` asserts
   `len(catalog['scenes'])==36`. Appending these scenes to `historical_scene_catalog.v1.json` would
   break that tool. Keep them in a sibling file, for example
   `docs/research/data/urban_intro_tableaux.v1.json`, or bump the catalog version and update the
   tool in the same commit.

## 7. Catalog-ready entries

The same fields as `historical_scene_catalog.v1.json`, with one added object, `urban_intro`. It holds:

- the scene's rank;
- its host typologies and rooms;
- its event and depicted windows;
- its inferred elements;
- its reference photographs.

```json
{
  "schema": "kessel.historical-scene-catalog.urban-intro",
  "version": 1,
  "extends": "assets/worldgen/scene_blocks/historical_scene_catalog.v1.json",
  "scenes": [
    {
      "id": "a1_cellar_embrasure_two_layers",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "The low window: cellar embrasure with an autumn gun and a winter machine gun",
      "priority": 1,
      "production_shortlist": true,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "cellar", "embrasure", "street", "tank_scale", "foot_scale", "two_layer"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_city_centre_winter", "stalingrad_urban_winter"],
        "requires": ["brick_building_with_cellar", "street_facade_plinth_windows", "straight_sightline_40m", "wreck_clear_of_vehicle_lane"],
        "forbids": ["factory_only", "steppe", "legible_markings", "named_landmark_replica", "live_looking_gun"]
      },
      "visual_beats": [
        "Semi-basement windows broken out into low slots face the street, pocking concentrated at ground level.",
        "A snow-capped German tank from the September advance stands on the slots' line of fire.",
        "Inside, a winter machine-gun slit narrowed with brick and a sandbag sits beside the shattered autumn 45 mm gun.",
        "Frozen cartridge cases and a cut wire line carry the residue outside."
      ],
      "source_records": [
        {"id": "plakolb_oesta_kab1540", "citation": "Max Plakolb, Luftwaffe liaison officer with 24th Panzer Division, September 1942, ÖStA-KA B/1540, quoted in Beevor, Stalingrad (1998), ch. 10, p. 149.", "url": "https://erenow.org/ww/stalingrad/11.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Contemporary German report reached through Beevor; access copy of the book."},
        {"id": "gurkin1963_pavlov_house", "citation": "Pavlov's House garrison arrangements (embrasures, mines, wire, trench, buried telephone line), ru.wikipedia citing Gurkin 1963; Beevor ch. 12.", "url": "https://ru.wikipedia.org/wiki/%D0%94%D0%BE%D0%BC_%D0%9F%D0%B0%D0%B2%D0%BB%D0%BE%D0%B2%D0%B0", "record_type": "encyclopedia_secondary", "framing": "Soviet-era account of a later-mythologised episode; physical arrangements only."},
        {"id": "hellbeck2015_38mrb_morozov", "citation": "Capt. L. P. Morozov and Capt. I. Z. Bukharov, 38th Motor Rifle Brigade, Historical Commission interviews 1943, in Hellbeck, Stalingrad: The City That Defeated the Third Reich (2015), 'The Capture of Field Marshal Paulus'.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php", "record_type": "state_organized_witness_interview", "framing": "Near-contemporary Soviet commission interview in edited translation; access copy."}
      ],
      "confidence": {"level": "high", "basis": "Cellar firing ports on both sides are attested by independent German and Soviet sources; the two-layer composition in one cellar is an authoring inference."},
      "caveats": ["Never replicate or name Pavlov's House or either L-shaped building.", "The static gun must read as wrecked evidence, not a live threat."],
      "urban_intro": {
        "rank": 1,
        "host_typologies": ["merchant_brick_house", "apartment_block"],
        "host_rooms": ["cellar", "street"],
        "event_window": "1942-09/1942-10; 1943-01-20/1943-01-31",
        "depicted_window": "1943-01-20/1943-01-31",
        "inferred": ["both layers in one cellar", "tank type", "brick-and-sandbag narrowing", "wreck position"],
        "reference_photos": ["08-13", "07-13", "07-07", "03-03", "03-01", "08-12", "07-12"]
      }
    },
    {
      "id": "a1_red_cross_cellar",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "The red cross over the cellar stair",
      "priority": 1,
      "production_shortlist": true,
      "provenance_class": "district_event",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "cellar", "dressing_station", "unburied_dead", "tank_scale", "foot_scale"],
      "placement_constraints": {
        "mobility": "district_only",
        "biomes": ["stalingrad_city_centre_winter"],
        "requires": ["cellar", "cellar_entrance_from_street_or_yard", "yard_or_street_wall_for_the_dead"],
        "forbids": ["factory_district", "airfield", "steppe", "legible_labels", "casualty_count"]
      },
      "visual_beats": [
        "A red-cross cloth hangs by a cellar arch where sledge runners and drag marks converge.",
        "A short row of the dead lies covered along the wall outside because the ground is frozen.",
        "Below, straw and blankets fill a vault lit only by tin wick lamps, a pressure lamp and wired vehicle headlamps.",
        "Figures left under blankets at the far wall remain unresolved."
      ],
      "source_records": [
        {"id": "paulus_1943_01_24_radio", "citation": "Paulus radio report, 24 January 1943: '18,000 wounded without the slightest aid of bandages and medicines' (Adam and Ruhle 2015, p. 193).", "url": "https://en.wikipedia.org/wiki/Battle_of_Stalingrad", "record_type": "encyclopedia_secondary", "framing": "Contemporary signal quoted through a secondary summary; never encode the figure."},
        {"id": "beevor1998_ch22_cellars", "citation": "Beevor, Stalingrad (1998), ch. 22: 20,000 wounded in city cellars; theatre cellars (Dr Hubert Haidinger, 1992); NKVD-building dressing station with red-cross flag and gas lamp (Dibold, Arzt in Stalingrad, 1949, pp. 24-5); frozen corpses outside the Tsaritsa tunnels (Achleitner).", "url": "https://erenow.org/ww/stalingrad/23.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Retrospective German medical accounts synthesised by Beevor; access copy."},
        {"id": "beevor1998_ch21_paper_bandages", "citation": "Beevor, Stalingrad (1998), ch. 21: limbs bound with paper (Dr Günther Diez); the non-movable left with a doctor and an orderly.", "url": "https://erenow.org/ww/stalingrad/22.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Access copy."},
        {"id": "hellbeck2015_38mrb_hospitals", "citation": "Morozov, Burmakov, Yegorov and Duka, 38th Motor Rifle Brigade, Historical Commission interviews 1943: German hospitals in the candy-factory and Univermag basements; basement lit by car batteries and headlights; 28-30 January 1943.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php", "record_type": "state_organized_witness_interview", "framing": "Near-contemporary Soviet commission interviews in edited translation; access copy."},
        {"id": "hellbeck2015_aksyonov_theatre", "citation": "Capt. Nikolai Aksyonov, 24 February 1943, City Theater hospital: 'the sick, the wounded, and the dead' together.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/28.php", "record_type": "state_organized_witness_interview", "framing": "Near-contemporary Soviet commission interview; access copy."}
      ],
      "confidence": {"level": "high", "basis": "Multiple independent Soviet and German accounts of cellar hospitals in the city centre in the last fortnight; props partly inferred."},
      "caveats": ["The shooting of wounded after the surrender is attested but must not be staged or implied.", "Build the outdoor version if cellars are not reachable; never paint an interior.", "Do not merge with a1_factory_cellar_ward."],
      "urban_intro": {
        "rank": 2,
        "host_typologies": ["merchant_brick_house", "apartment_block"],
        "host_rooms": ["cellar", "stairwell", "yard", "street"],
        "event_window": "1943-01-20/1943-01-31",
        "depicted_window": "1943-01-20/1943-01-31",
        "inferred": ["building type", "door-on-trestles table", "tin flue", "flag pole", "ambiguous remaining figures", "number of dead"],
        "reference_photos": ["07-13", "04-20", "07-11", "04-11", "06-13", "07-04", "09-13"]
      }
    },
    {
      "id": "a1_chimney_field_dugout",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "Smoke from the ground: burned timber quarter with a family dugout",
      "priority": 1,
      "production_shortlist": true,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_residential", "timber_fringe", "civilian_survival", "dugout", "yard", "foot_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_urban_edge_winter", "stalingrad_urban_winter"],
        "requires": ["burned_wooden_house_plots_min_3", "yard", "modelled_fire_source"],
        "forbids": ["intact_timber_house", "summer", "legible_text", "posed_figures"]
      },
      "visual_beats": [
        "Brick stoves and chimneys stand alone at plot pitch over ash and fence posts stripped of boards.",
        "Between two stoves a dugout roofed with doors and sheet iron breathes smoke from a tin-can stovepipe.",
        "A small sledge, a sack and a bucket wait at the blanketed entrance.",
        "A trodden path leads away toward water."
      ],
      "source_records": [
        {"id": "hellbeck2015_city_residents", "citation": "Historical Commission interviews (Pigalyov, Vodolagin, Denisova, Ioffe), 1943-44, in Hellbeck (2015), 'The Fate of the City and Its Residents'.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/17.php", "record_type": "state_organized_witness_interview", "framing": "Near-contemporary; access copy."},
        {"id": "hellbeck2015_pozdnyakova_en", "citation": "Agrafena Pozdnyakova, interview of 14 March 1943, and Hellbeck's introduction (15,000 civilians in German-held areas; ransacking; wood commandos).", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php", "record_type": "state_organized_witness_interview", "framing": "Named civilian witness; access copy."},
        {"id": "beevor1998_ch24_census", "citation": "Beevor, Stalingrad (1998), ch. 24: census of at least 9,796 civilians including 994 children (RTsKhIDNI 17/8/226); children 'living in the ground' (Wettlin).", "url": "https://erenow.org/ww/stalingrad/25.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Never encode the census figure."},
        {"id": "barch_183_b29644", "citation": "Bundesarchiv Bild 183-B29644, September 1942: residents with salvaged belongings among burned wooden houses.", "url": "https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-B29644,_Zivilbev%C3%B6lkerung_im_zerst%C3%B6rten_Stalingrad.jpg", "record_type": "archive_photograph", "framing": "ADN caption; form reference."},
        {"id": "nac_2_939", "citation": "NAC 2-939 (Bauer, German PK), October 1942: plank-framed shelter entrances in a city garden.", "url": "https://commons.wikimedia.org/wiki/File:Stalingrad._Zniszczone_schrony_i_ruiny_miasta._(2-939).jpg", "record_type": "archive_photograph", "framing": "German PK photograph; form reference."}
      ],
      "confidence": {"level": "high", "basis": "Burned timber quarters and civilians living in the ground are attested by photographs, commission interviews and the post-battle census."},
      "caveats": ["Do not restage Pozdnyakova's September fire (a1_burned_cellar_household).", "No chalked messages: they are text."],
      "urban_intro": {
        "rank": 3,
        "host_typologies": ["wooden_house"],
        "host_rooms": ["yard"],
        "event_window": "1942-08-23/1943-02-02",
        "depicted_window": "1943-01",
        "inferred": ["dugout construction", "hoof on the floor", "path target", "plot choice"],
        "reference_photos": ["04-27", "07-14", "04-22", "06-13", "09-06"]
      }
    },
    {
      "id": "a1_tram_on_its_rails",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "The tram that stopped on 23 August",
      "priority": 1,
      "production_shortlist": true,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "street", "tram", "draft_horse", "tank_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_city_centre_winter", "stalingrad_urban_winter"],
        "requires": ["tram_rails_1524mm", "street_lane_6m_clear"],
        "forbids": ["trolleybus", "twin_trolley_wire", "legible_route_boards", "horse_on_vehicle_line"]
      },
      "visual_beats": [
        "A two-axle Kh tram car stands on its rails with every pane gone and its side panels holed.",
        "A double-bracket pole still carries span wires; one has fallen into a sagging arc.",
        "A frozen dead horse lies on its side beside the line in trampled snow.",
        "A crate and a bucket lie half-sunk nearby."
      ],
      "source_records": [
        {"id": "kopyt_1943_02_tram", "citation": "Yefim Kopyt, February 1943, Tulak Street (today Bazisnaya Street): 'a tram carcass riddled by bullets and fragments in the central part of Stalingrad'; Stalingrad Battle Museum-Reserve.", "url": "https://commons.wikimedia.org/wiki/File:%D0%9D%D0%B0_%D0%BE%D0%B4%D0%BD%D0%BE%D0%B9_%D0%B8%D0%B7_%D1%83%D0%BB%D0%B8%D1%86_%D0%A1%D1%82%D0%B0%D0%BB%D0%B8%D0%BD%D0%B3%D1%80%D0%B0%D0%B4%D0%B0.jpg", "record_type": "museum_photograph", "framing": "Wartime Soviet photograph; physical reference, not the location."},
        {"id": "ruwiki_volgograd_tram", "citation": "Stalingrad tram: service until 23 August 1942, network destroyed during the battle, Kh cars from 1929, 1,524 mm gauge.", "url": "https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%B0%D0%BC%D0%B2%D0%B0%D0%B9", "record_type": "encyclopedia_secondary", "framing": "Local transport history."},
        {"id": "ruwiki_kh_tram", "citation": "Kh motor car: two axles, body 9,565 mm (10,270 over buffers), 2,464 mm wide, 3,274 mm rail-to-roof, 12.65 t, steel frame and panels, wooden roof and frames.", "url": "https://ru.wikipedia.org/wiki/%D0%A5_(%D1%82%D1%80%D0%B0%D0%BC%D0%B2%D0%B0%D0%B9%D0%BD%D1%8B%D0%B9_%D0%B2%D0%B0%D0%B3%D0%BE%D0%BD)", "record_type": "encyclopedia_secondary", "framing": "Vehicle data."},
        {"id": "ruwiki_volgograd_trolleybus", "citation": "Stalingrad trolleybus opened 31 December 1960.", "url": "https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%BE%D0%BB%D0%BB%D0%B5%D0%B9%D0%B1%D1%83%D1%81", "record_type": "encyclopedia_secondary", "framing": "Refutes trolleybuses in 1942-43."}
      ],
      "confidence": {"level": "high", "basis": "A dated museum photograph of the arrangement plus the tram network's documented stop on 23 August 1942."},
      "caveats": ["The photograph does not date this car's stopping; the network's stop does.", "Keep the horse whole; butchering belongs to the catalog's horse scenes."],
      "urban_intro": {
        "rank": 4,
        "host_typologies": ["merchant_brick_house", "wooden_house", "apartment_block"],
        "host_rooms": ["street"],
        "event_window": "1942-08-23/1943-02",
        "depicted_window": "1943-01-20/1943-01-31",
        "inferred": ["car position", "fallen wire", "knocking frame"],
        "reference_photos": ["08-12", "08-13", "04-23", "03-04"]
      }
    },
    {
      "id": "a1_white_canopy_canister",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "White canopies: air-drop canister in the ruins",
      "priority": 1,
      "production_shortlist": true,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "air_supply", "open_ground", "sniper_exposure", "tank_scale", "foot_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_city_centre_winter", "stalingrad_urban_winter"],
        "requires": ["tall_gutted_block", "open_square_or_yard_20m_to_cover"],
        "forbids": ["absurd_cargo", "legible_stencils", "posed_body", "steppe"]
      },
      "visual_beats": [
        "A white canopy snagged high on a gutted block trails its lines down the facade.",
        "An unopened canister lies half-sunk in the open between ruins.",
        "A second canister near a cellar mouth is split and empty.",
        "One line of tracks toward the unopened canister ends at a low mound under snow."
      ],
      "source_records": [
        {"id": "beevor1998_ch22_supply_bombs", "citation": "Beevor, Stalingrad (1998), ch. 22: white canopies despite requests for red (Behr, 1995); cross-formation order (BA-MA RH19 VI/12, p. 451); snipers picked off men retrieving canisters in the open.", "url": "https://erenow.org/ww/stalingrad/23.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Access copy."},
        {"id": "hellbeck2015_pist_1943_02_09", "citation": "Sgt Helmut Pist, 24th Panzer Division, interrogation 9 February 1943: air-dropped food taken by whoever found it despite the threat of being shot.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/32.php", "record_type": "pow_interrogation", "framing": "Captive context."},
        {"id": "adam_ruhle_drops_after_0123", "citation": "After 23 January no landings, only intermittent drops (Adam and Ruhle 2015, pp. 183-189).", "url": "https://en.wikipedia.org/wiki/Battle_of_Stalingrad", "record_type": "encyclopedia_secondary", "framing": "Chronology."}
      ],
      "confidence": {"level": "high", "basis": "Independent German and captive testimony on drop failures and exposure; canister type is form reference only."},
      "caveats": ["Canister type is not identified for the city; use the photograph's form without claiming the model.", "If the canopy moves, drive it by the district's wind state."],
      "urban_intro": {
        "rank": 5,
        "host_typologies": ["apartment_block"],
        "host_rooms": ["yard", "street"],
        "event_window": "1943-01-23/1943-02-02",
        "depicted_window": "1943-01-23/1943-01-31",
        "inferred": ["canister type", "two canisters", "the mound", "tool marks"],
        "reference_photos": ["05-07", "04-09", "04-01", "06-12", "04-11"]
      }
    },
    {
      "id": "a1_basement_with_stovepipe",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "The basement with the stovepipe: civilians sheltering German wounded",
      "priority": 2,
      "production_shortlist": false,
      "provenance_class": "district_event",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "cellar", "civilian_survival", "wounded", "foot_scale"],
      "placement_constraints": {
        "mobility": "district_only",
        "biomes": ["stalingrad_city_centre_winter"],
        "requires": ["domestic_cellar", "exterior_stovepipe", "modelled_fire_source"],
        "forbids": ["factory_district", "faces", "legible_text", "within_100m_of_a1_red_cross_cellar"]
      },
      "visual_beats": [
        "Smoke rises from a tin stovepipe pushed out of a semi-basement window.",
        "One cellar holds a household's corner and a corner of soldiers' greatcoats and helmets.",
        "Rags in a basin and a pan of water on the stove show wounds cleaned.",
        "A flat loaf and a lump of frozen horsemeat lie on boards; no one remains."
      ],
      "source_records": [
        {"id": "hellbeck2015_duka_basement", "citation": "Jr Sgt Alexander S. Duka, 38th Motor Rifle Brigade, Historical Commission interview 1943: smoke from a chimney; an old man and a woman in a basement with eleven Germans, five wounded; the woman cleaned and bandaged their wounds.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php", "record_type": "state_organized_witness_interview", "framing": "Near-contemporary; access copy."},
        {"id": "beevor1998_ch22_gebele", "citation": "Gebele, 297th Infantry Division, retrospective, in Beevor ch. 22: women rubbed frozen legs; bread traded for frozen horsemeat.", "url": "https://erenow.org/ww/stalingrad/23.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Retrospective German account."}
      ],
      "confidence": {"level": "medium_high", "basis": "One precise Soviet account with an independent German account of the same kind of relationship."},
      "caveats": ["The killings on the basement stairs in Duka's account are not staged.", "Never show sexual violence, which Hellbeck's introduction attests."],
      "urban_intro": {
        "rank": 6,
        "host_typologies": ["merchant_brick_house", "wooden_house"],
        "host_rooms": ["cellar", "yard"],
        "event_window": "1943-01-28/1943-01-29",
        "depicted_window": "1943-01-28/1943-01-31",
        "inferred": ["house type", "two-corner layout", "objects per corner", "absence of people"],
        "reference_photos": ["07-11", "04-20", "09-06", "07-13", "04-11"]
      }
    },
    {
      "id": "a1_floor_breach_strongpoint",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "Through the floor: a German-held block cleared from above",
      "priority": 2,
      "production_shortlist": false,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "apartment_block", "cellar", "assault_traces", "sap_trench", "foot_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_city_centre_winter", "stalingrad_urban_winter"],
        "requires": ["apartment_block_with_cellar", "open_field_30_50m", "failed_slab_module_above_cellar"],
        "forbids": ["burned_bodies", "named_landmark", "slogans", "paint_over_damage"]
      },
      "visual_beats": [
        "A narrow sap without spoil heaps reaches to within 20-30 m of the block across open ground.",
        "A ragged hole through the ground floor opens into the cellar, soot fanning up the walls.",
        "Elsewhere a floor has dropped into the cellar, joists hanging.",
        "The cellar is black, its embrasures still facing the street, and empty of bodies."
      ],
      "source_records": [
        {"id": "hellbeck2015_rodimtsev", "citation": "Maj. Gen. Alexander Rodimtsev, 13th Guards Rifle Division, Historical Commission interview: saps to 20-30 m; ceiling broken with crowbars and flamethrowers; 250 kg TNT on the floor above a cellar; the L-shaped house, 3 December 1942.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/24.php", "record_type": "state_organized_witness_interview", "framing": "Near-contemporary; access copy."},
        {"id": "hellbeck2015_38mrb_morozov", "citation": "Morozov: German basement machine guns and embrasures, L-shaped building in the southern centre, 29-30 January 1943.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php", "record_type": "state_organized_witness_interview", "framing": "Near-contemporary; access copy."},
        {"id": "beevor1998_ch22_cellars_cleared", "citation": "Beevor, Stalingrad (1998), ch. 22: by 31 January ruined buildings and cellars cleared with grenade and flame-thrower.", "url": "https://erenow.org/ww/stalingrad/23.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Access copy."}
      ],
      "confidence": {"level": "high", "basis": "Commander's own account of the methods, supported by a second assault account and the general record."},
      "caveats": ["Do not name or replicate the L-shaped house or the railway workers' house.", "The breach must be a structural state of the building model."],
      "urban_intro": {
        "rank": 7,
        "host_typologies": ["apartment_block"],
        "host_rooms": ["stairwell", "dwelling", "cellar", "yard", "street"],
        "event_window": "1942-12-03; 1943-01-29/1943-01-31",
        "depicted_window": "1943-01-29/1943-01-31",
        "inferred": ["hole size", "charred objects", "sap route", "two methods in one building"],
        "reference_photos": ["04-09", "04-11", "07-07", "04-02", "04-12", "04-04"]
      }
    },
    {
      "id": "a1_kitchen_firing_post",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "The kitchen chair at the shell hole",
      "priority": 2,
      "production_shortlist": false,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_residential", "upper_floor", "observation_post", "domestic_loss", "foot_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_city_centre_winter", "stalingrad_urban_winter"],
        "requires": ["upper_floor_dwelling", "shell_hole_facing_80m_sightline"],
        "forbids": ["legible_papers", "identifiable_photographs", "body_at_the_post"]
      },
      "visual_beats": [
        "A kitchen chair faces a shell hole in the outer wall of an upper-floor flat, visible from the street.",
        "Cartridge cases lie under the hole and a cut telephone cable runs down the stairwell.",
        "An iron bedstead, a lamp, broken crockery and a vase of dead flowers remain.",
        "Snow blown through the hole lies in a fan across chair and floor."
      ],
      "source_records": [
        {"id": "grossman_rgali_618_2_108", "citation": "Vasily Grossman, notes (RGALI 618/2/108), in Beevor, Stalingrad (1998), ch. 10, p. 148: withered flowers and homework in fought-over flats; a spotter at a shell hole on a kitchen chair.", "url": "https://erenow.org/ww/stalingrad/11.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Wartime notes reached through Beevor; access copy."},
        {"id": "barch_101i_617_2571_04", "citation": "Bundesarchiv Bild 101I-617-2571-04 (Ollig), 23 September 1942: German MG 34 team firing from a flat's window.", "url": "https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_101I-617-2571-04,_Stalingrad,_Soldaten_beim_H%C3%A4userkampf.jpg", "record_type": "archive_photograph", "framing": "PK photograph; form reference."}
      ],
      "confidence": {"level": "medium_high", "basis": "Wartime notes and photographs attest the practice; the specific furnishing is inferred."},
      "caveats": ["The exercise book stays closed with a blank cover."],
      "urban_intro": {
        "rank": 8,
        "host_typologies": ["apartment_block", "merchant_brick_house"],
        "host_rooms": ["dwelling", "stairwell"],
        "event_window": "1942-09/1943-01",
        "depicted_window": "1943-01",
        "inferred": ["furniture", "binocular rest", "case calibre", "storey"],
        "reference_photos": ["07-12", "07-08", "04-02", "04-09", "04-11"]
      }
    },
    {
      "id": "a1_evicted_household_yard",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "Turned out into the yard",
      "priority": 3,
      "production_shortlist": false,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_residential", "occupation", "civilian_survival", "yard_trench", "foot_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_urban_winter", "stalingrad_urban_edge_winter"],
        "requires": ["roofed_house", "yard_visible_from_street", "slit_trench"],
        "forbids": ["violence", "faces", "unit_signs", "within_150m_of_a1_chimney_field_dugout"]
      },
      "visual_beats": [
        "One of the few roofed houses shows occupation: a telephone cable through a window and blanketed panes.",
        "In the yard a slit trench roofed with a door holds bundles, a trunk, bedding and a child's sledge.",
        "Frost and fresh snow lie on everything the family could carry.",
        "Nothing shows what became of them."
      ],
      "source_records": [
        {"id": "hellbeck2015_pozdnyakova_eviction", "citation": "Agrafena Pozdnyakova, interview of 14 March 1943: on 26 January the Germans made her building their headquarters and put the family into the courtyard trenches at 4 a.m. for two days; Hellbeck's introduction on soldiers taking undamaged residences.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php", "record_type": "state_organized_witness_interview", "framing": "Named civilian witness; editorial summary from Soviet documents; access copy."}
      ],
      "confidence": {"level": "medium_high", "basis": "One named testimony with a documentary summary of the general practice."},
      "caveats": ["Pozdnyakova's family survived; do not imply their deaths."],
      "urban_intro": {
        "rank": 9,
        "host_typologies": ["wooden_house", "merchant_brick_house"],
        "host_rooms": ["yard", "dwelling"],
        "event_window": "1942-12/1943-01-28",
        "depicted_window": "1943-01-26/1943-01-31",
        "inferred": ["occupation signs", "trench roof", "objects", "telephone bell"],
        "reference_photos": ["03-04", "06-05", "07-14"]
      }
    },
    {
      "id": "a1_yard_of_the_unburied",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "Where the ground would not open",
      "priority": 3,
      "production_shortlist": false,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "unburied_dead", "yard", "foot_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_city_centre_winter", "stalingrad_urban_winter"],
        "requires": ["yard_wall", "off_vehicle_route"],
        "forbids": ["gore", "faces", "precise_body_count", "burial_parties", "cannibalism"]
      },
      "visual_beats": [
        "The dead lie in a covered line along a yard wall, some without boots.",
        "A pick and shovel lie abandoned beside a shallow scrape in frozen ground.",
        "At the gate, older mounds with a birch cross and a helmet hold the autumn dead.",
        "A small tin by the wall holds broken tag halves."
      ],
      "source_records": [
        {"id": "beevor1998_ch20_frozen_ground", "citation": "Beevor, Stalingrad (1998), ch. 20: frozen corpses left at Pitomnik because the ground was frozen too hard to bury them (Morzik, p. 188).", "url": "https://erenow.org/ww/stalingrad/21.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Access copy."},
        {"id": "beevor1998_ch21_tags_clothes", "citation": "Beevor, Stalingrad (1998), ch. 21: corpses in heaps at Gumrak (Stolberg report, BA-MA RW4/v.264); chaplains snapped off tag halves; clothes stripped before bodies froze.", "url": "https://erenow.org/ww/stalingrad/22.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Access copy."},
        {"id": "hellbeck2015_pist_dead", "citation": "Sgt Helmut Pist, interrogation 9 February 1943: 'thousands of dead bodies, and the wounded men dying in the streets'.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/32.php", "record_type": "pow_interrogation", "framing": "Captive context."}
      ],
      "confidence": {"level": "high", "basis": "Several independent accounts of unburied dead at aid posts and roads inside the pocket."},
      "caveats": ["Best placed as the yard of a1_red_cross_cellar.", "Never encode a count."],
      "urban_intro": {
        "rank": 10,
        "host_typologies": ["apartment_block", "merchant_brick_house"],
        "host_rooms": ["yard"],
        "event_window": "1942-10; 1943-01-15/1943-01-31",
        "depicted_window": "1943-01-20/1943-01-31",
        "inferred": ["scrape and tools", "tin of tag halves", "gate mounds", "yard choice"],
        "reference_photos": ["05-12", "09-13", "04-11"]
      }
    },
    {
      "id": "a1_residents_barricade",
      "act": 1,
      "phase": "die_stadt",
      "historical_region": "stalingrad",
      "title": "The street the residents closed",
      "priority": 3,
      "production_shortlist": false,
      "provenance_class": "attested_condition",
      "compatibility_tags": ["stalingrad", "winter", "urban_centre", "street", "barricade", "autumn_layer", "tank_scale"],
      "placement_constraints": {
        "mobility": "compatible_biome",
        "biomes": ["stalingrad_city_centre_winter", "stalingrad_urban_winter"],
        "requires": ["street", "photographic_reference_before_build"],
        "forbids": ["posters", "slogans", "heroic_tableau", "named_street"]
      },
      "visual_beats": [
        "Steel hedgehogs lie across a street, two shoved aside and one on its back.",
        "A low bank of setts, earth and timber has a vehicle-wide gap pushed through it.",
        "Snow drifts against the gap's edges.",
        "Tram rails in the gap are bent."
      ],
      "source_records": [
        {"id": "beevor1998_ch8_barricade_order", "citation": "Beevor, Stalingrad (1998), ch. 8: state-of-siege posters and the Defence Committee order of 26 August 1942 to barricade every street (TsMVS).", "url": "https://erenow.org/ww/stalingrad/9.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Access copy."},
        {"id": "beevor1998_ch9_chuikov_barricades", "citation": "Beevor, Stalingrad (1998), ch. 9: Chuikov found barricades that could be pushed over with the front of a truck.", "url": "https://erenow.org/ww/stalingrad/10.php", "record_type": "secondary_synthesis_with_archival_notes", "framing": "Access copy."},
        {"id": "hellbeck2015_tank_traps", "citation": "Hellbeck (2015), 'The Fate of the City and Its Residents': Red October made tank traps after the state of siege; Denisova's 14 September report on barricade construction.", "url": "https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/17.php", "record_type": "state_organized_witness_interview", "framing": "Access copy."}
      ],
      "confidence": {"level": "medium", "basis": "Barricades and tank traps are attested in text; their physical form is not described in the sources found."},
      "caveats": ["Do not build until a dated photograph of a Stalingrad street barricade is found."],
      "urban_intro": {
        "rank": 11,
        "host_typologies": ["merchant_brick_house", "apartment_block", "wooden_house"],
        "host_rooms": ["street"],
        "event_window": "1942-08-26/1942-09-14",
        "depicted_window": "1943-01",
        "inferred": ["physical form of the barricade", "hedgehog count", "bent rails"],
        "reference_photos": []
      }
    }
  ]
}
```

## 8. Sources

Books; the erenow pages are unofficial access copies, so metadata cites the printed editions:

1. Jochen Hellbeck, *Stalingrad: The City That Defeated the Third Reich* (PublicAffairs, 2015),
   translated by Christopher Tauchen and Dominic Bonfiglio. Sections used:
   - "The Fate of the City and Its Residents":
     <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/17.php>
   - "Agrafena Pozdnyakova": <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php>
   - "The Capture of Field Marshal Paulus":
     <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/22.php>
   - "Guards Division General Alexander Rodimtsev":
     <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/24.php>
   - "Nurse Vera Gurova": <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/25.php>
   - "The History Instructor: Captain Nikolai Aksyonov":
     <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/28.php>
   - "German Prisoners in February 1943":
     <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/32.php>
   - "A German Diary from the Kessel":
     <https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/33.php>
   - Russian edition (НЛО, 2015):
     <https://www.nlobooks.ru/books/biblioteka_zhurnala_neprikosnovennyy_zapas/19217/>
2. Antony Beevor, *Stalingrad* (Viking, 1998):

   | Chapter | Page |
   |---|---|
   | ch. 8 | <https://erenow.org/ww/stalingrad/9.php> |
   | ch. 9 | <https://erenow.org/ww/stalingrad/10.php> |
   | ch. 10 | <https://erenow.org/ww/stalingrad/11.php> |
   | ch. 12 | <https://erenow.org/ww/stalingrad/13.php> |
   | ch. 18 | <https://erenow.org/ww/stalingrad/19.php> |
   | ch. 19 | <https://erenow.org/ww/stalingrad/20.php> |
   | ch. 20 | <https://erenow.org/ww/stalingrad/21.php> |
   | ch. 21 | <https://erenow.org/ww/stalingrad/22.php> |
   | ch. 22 | <https://erenow.org/ww/stalingrad/23.php> |
   | ch. 24 | <https://erenow.org/ww/stalingrad/25.php> |
   | Source notes | <https://erenow.org/ww/stalingrad/30.php> and <https://erenow.org/ww/stalingrad/31.php> |

Encyclopedias:

3. English Wikipedia, "Battle of Stalingrad" (Adam and Ruhle 2015, Beevor 1998 and others cited
   there): <https://en.wikipedia.org/wiki/Battle_of_Stalingrad>
4. Russian Wikipedia:

   | Article | URL |
   |---|---|
   | Волгоградский трамвай | <https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%B0%D0%BC%D0%B2%D0%B0%D0%B9> |
   | Волгоградский троллейбус | <https://ru.wikipedia.org/wiki/%D0%92%D0%BE%D0%BB%D0%B3%D0%BE%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B8%D0%B9_%D1%82%D1%80%D0%BE%D0%BB%D0%BB%D0%B5%D0%B9%D0%B1%D1%83%D1%81> |
   | Х (трамвайный вагон) | <https://ru.wikipedia.org/wiki/%D0%A5_(%D1%82%D1%80%D0%B0%D0%BC%D0%B2%D0%B0%D0%B9%D0%BD%D1%8B%D0%B9_%D0%B2%D0%B0%D0%B3%D0%BE%D0%BD)> |
   | Дом Павлова | <https://ru.wikipedia.org/wiki/%D0%94%D0%BE%D0%BC_%D0%9F%D0%B0%D0%B2%D0%BB%D0%BE%D0%B2%D0%B0> |
   | Сталинградская битва | <https://ru.wikipedia.org/wiki/%D0%A1%D1%82%D0%B0%D0%BB%D0%B8%D0%BD%D0%B3%D1%80%D0%B0%D0%B4%D1%81%D0%BA%D0%B0%D1%8F_%D0%B1%D0%B8%D1%82%D0%B2%D0%B0> |

Photographs outside the reference set, all on Wikimedia Commons:

5. Yefim Kopyt, tram in a Stalingrad street, February 1943:
   <https://commons.wikimedia.org/wiki/File:%D0%9D%D0%B0_%D0%BE%D0%B4%D0%BD%D0%BE%D0%B9_%D0%B8%D0%B7_%D1%83%D0%BB%D0%B8%D1%86_%D0%A1%D1%82%D0%B0%D0%BB%D0%B8%D0%BD%D0%B3%D1%80%D0%B0%D0%B4%D0%B0.jpg>.
   The exhibition report is at <https://v1.ru/text/culture/2017/12/22/53300861/>.
6. Kh tram at the tractor-factory terminus, 1930s:
   <https://commons.wikimedia.org/wiki/File:Tramway_Stalingrad_STZ.jpg>
7. NAC 2-939, shelters in a city garden, October 1942:
   <https://commons.wikimedia.org/wiki/File:Stalingrad._Zniszczone_schrony_i_ruiny_miasta._(2-939).jpg>
8. Bundesarchiv Bild 183-B29644, residents amid burned houses, September 1942:
   <https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-B29644,_Zivilbev%C3%B6lkerung_im_zerst%C3%B6rten_Stalingrad.jpg>
9. For the Univermag and the 23 August raid:
   - Bundesarchiv Bild 183-B22531, the department-store ruin, October 1942:
     <https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-B22531,_Stalingrad,_Ruine_des_Warenhauses.jpg>
   - Bundesarchiv Bild 183-B22081, the air raid of August 1942:
     <https://commons.wikimedia.org/wiki/File:Bundesarchiv_Bild_183-B22081,_Russland,_Kampf_um_Stalingrad,_Luftangriff.jpg>

Project files:

10. The reference set: `/home/alex/panzer/reference/wartime_photos_20260928/`
    (`manifest.json` and README).
11. P's analysis: `/home/alex/.claude/jobs/ca78baf9/tmp/research/PHOTO_REFERENCE_ANALYSIS_20260928.md`.
    Its photograph identifiers are the `[FF-NN]` identifiers used above.
12. Repository context:
    - `docs/research/HISTORICAL_SCENE_SOURCE_AUDIT_20260925.md`
    - `docs/research/data/stalingrad.json` (landmark coordinates)
    - `tools/worldgen/gen_world.py`
    - `assets/terrain/stalingrad_100km_meta.json`
