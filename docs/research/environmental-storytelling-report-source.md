# Historical Scene Dossier — Canonical Research Source

Status: production research source  
Internal phases: **Die Stadt** (Stalingrad); **Die Heimkehr** (Oderbruch, Seelow, and Küstrin); **Kameraden** (Halbe and the Elbe). These phase names are production metadata and are not announced to the player.  
Companion data: `assets/worldgen/scene_blocks/historical_scene_catalog.v1.json`

## Doctrine

**Der Panzerrechnersimulator is an environmental-storytelling game that tells one coherent story without a single spoken or written word, exclusively through the environments the player explores.**

Its scene blocks are not generic “war atmosphere.” Every scene must be grounded in a specific real event or a well-attested material condition. Horror comes from the truthful arrangement of ordinary things after systems, homes, vehicles, and bodies have failed—not from invented spectacle. The anti-war meaning must remain legible through space, damage, absence, weather, sound, and the changed use of everyday objects.

Developer titles, source metadata, and placement notes may contain text. The player-facing scene may not rely on readable letters, diaries, captions, signs, objective text, or dialogue. A paper prop may communicate only through its non-linguistic form: fold, route line, stamp color, tear, pin, placement, or relation to other objects.

## Historical-integrity boundary

**Source truth attaches to each tableau. It does not automatically attach to the player’s Tiger, the route between tableaux, their adjacency, or the campaign’s compressed three-phase chronology.** The journey is a fictional composite unless a separate vehicle-and-route study validates a connection. Never imply that one Tiger crew witnessed every cataloged event, that two movable blocks historically stood next to one another, or that their order in a generated playthrough reproduces real chronology.

An `exact_landmark` fixes a scene to its own attested site; it does not prove the fictional crew reached that site. A `district_event` or `attested_condition` preserves the sourced material relationship while openly surrendering an exact coordinate. Campaign routing, vehicle identity, unit continuity, travel time, front-line accessibility, and cross-phase chronology require their own provenance layer and remain outside this dossier.

## Method and limits

This dossier is a bounded production synthesis of the supplied research lanes. It prioritizes contemporary interviews, diaries, reports, photographs, aerial photographs, and direct observations. Archaeology and official histories are used to locate or interpret physical evidence. Retrospective testimony is retained only with an explicit caveat. No scene below should be expanded with “plausible” detail unless a new source record is added.

Primary does not mean neutral. The Stalingrad interviews were gathered by a Soviet state commission; several German diaries survive here through captured wartime translations; POW interrogations were coercive contexts; Alexander Werth worked under controlled access; Soviet and German records had operational and propaganda purposes; archive captions can be incomplete; retrospective testimony can compress or misremember events. Those conditions are recorded rather than hidden.

Where casualty estimates conflict, this catalog encodes the physical condition—unburied dead, crowded wards, scattered wounded, fresh graves—without literalizing a disputed total. Where a photograph is only tentatively attributed, its geometry is reference-only. Where a testimony links separate households or moments, the production scene must preserve that separation.

### Provenance classes

| Class | Meaning | Placement rule |
|---|---|---|
| `exact_landmark` | A named structure, earthwork, bridge, or site is directly attested. | Keep the authored block at that landmark or an explicitly declared campaign-scale representation of it. It is not a freely movable encounter tile. |
| `district_event` | The event is tied to a named district, route, airfield, fortress, town, or battlefield zone, but the exact point is uncertain or broad. | Move only within that geographic and biome envelope; preserve approach direction and required neighboring features. |
| `attested_condition` | The material condition is documented but not securely tied to one exact point, or recurred across the campaign area. | It may be placed in any compatible biome and phase, but must never be presented as the exact photographed or witnessed location. |

## Production shortlist

These twelve blocks give the first production pass a strong rhythm of home, logistics, command, medical collapse, battlefield geography, and aftermath. They are the `priority: 1` entries in the machine-readable catalog.

| Act | Scene | Why it leads |
|---|---|---|
| I | A1-01 Burned cellar household | One residential block carries shelter, fire, flight, trenches, and family absence without exposition. |
| I | A1-03 Grain-elevator extraction convoy | A landmark-scale logistics scene connects occupation, food, coerced labor, and confiscation. |
| I | A1-07 Pitomnik failed-evacuation airfield | Terrain, aircraft, aid bunkers, and an unusable runway make systemic collapse readable at vehicle scale. |
| I | A1-10 Red Army House frozen courtyard | A directly observed, exact urban landmark provides a stark post-surrender endpoint. |
| II | A2-01 Floodwater-hidden wooden crossing | The spring floodplain becomes story, cover, traversal problem, and engineering evidence at once. |
| II | A2-05 “Küstrin Pompeii” street block | Intact street geometry around almost-total building loss makes absence the dominant image. |
| II | A2-08 Reitweiner Sporn command complex | Surveyed dugouts, vehicle shelters, observation posts, and communication trenches form a large outdoor scene block. |
| II | A2-11 Dog-drawn casualty evacuation | One contemporary photograph supports an intimate, non-heroic movement scene with a distinctive silhouette. |
| III | A3-04 Mapless Dahme bridge bottleneck | The campaign’s navigation theme becomes physical congestion, scarcity, and blocked terrain. |
| III | A3-06 Forsthaus Halbe after battle | An exact postbattle photograph can drive architecture, damage, debris, and approach composition. |
| III | A3-10 Immediate graves in gardens and craters | Domestic and military landscapes physically overlap without invented narrative. |
| III | A3-12 Tangermünde ruined-bridge crossing | A photographed final crossing gives the journey a visually unambiguous but wordless terminus. |

## Silent visual grammar

Each block should read as a causal chain of three to five beats, not as a prop scatter. Use these rules:

1. **Approach, threshold, focus, residue.** Establish what happened before the player reaches the focal object. Tracks, congestion, moved furniture, cut wire, or flood marks should lead into the scene; empty harnesses, ash, discarded containers, or filled craters should carry it past the focal point.
2. **Objects change function.** A town-hall stable becomes a weapons store; a garden becomes a grave field; furniture becomes fuel; a rail tanker becomes a water source; a sanatorium becomes a surrender ward. This functional inversion is the core visual sentence.
3. **Absence is evidence.** Empty harness, missing doors, a line of boots without their wearers, one intact chapel in a destroyed block, or an abandoned command cable can say more than another body prop.
4. **Repeat with variation.** Repeated buckets, shelter mouths, revetments, stretchers, wagon ruts, or graves establish scale. Do not clone one pose or one damage decal.
5. **Show sequence through material state.** Fresh mud over old snow, wet flood lines, warm ash beside frozen objects, successive track layers, scavenged interiors, and newer graves inside older craters create time without text.
6. **Make evidence visible from both tank and foot scale.** A 100-m silhouette or terrain deformation should invite approach; 1–10-m object relations should reward dismounting. Do not hide the only readable beat in a closed room.
7. **No invented hero tableau.** Do not pose anonymous figures into a cinematic story the sources do not establish. Use the attested physical relation and leave identities unresolved.
8. **No readable exposition.** Maps can show roads and shapes; passes can show color and damage; papers can be folded, burned, or pinned. Their words remain unreadable.

### Reusable prop vocabulary

- **Movement:** overlapping tracks, wheel ruts, drag marks, empty harness, broken traces, handcart, abandoned bicycle, compressed snow, churned mud.
- **Logistics:** sacks, buckets, cistern, crates, supply containers, cable reels, generators, vehicle shelters, revetments, discarded kit.
- **Care and bodily failure:** stretchers, aid-post entrances, improvised bedding, bandage remains, floor fire, empty medicine storage, evacuation cart, fresh graves.
- **Shelter and domestic loss:** cellar threshold, burned joists, stripped furniture, stove, bedding straw, buried box depression, roof tile, one surviving household object.
- **Command and disintegration:** field telephone, cut cable routes, observation slit, shelter trench, map plane with non-readable linework, surrendered weapons separated from their owners.
- **Aftermath:** ash, flood marks, crater reuse, blocked road, overlapping wrecks, façade islands, unburied or recently buried dead represented without a disputed count.

## Die Stadt — Stalingrad (internal Act I)

Act I supports five placement envelopes: burned residential blocks; rail and warehouse corridors; named city landmarks and factory belts; exposed steppe roads; and the Gumrak/Pitomnik airfield districts. Snow, frozen earth, rubble, cellars, and the Volga-side industrial fabric are mandatory context. Do not float a grain-elevator, Red October, Red Army House, or sanatorium scene into a generic village. Airfield scenes require runway geometry and exposed artillery sightlines; factory-cellar scenes require the industrial/Volga district, not a domestic basement.

### A1-01 — Burned cellar household

- **Class / placement:** `district_event`; Solnechnaya Street residential block, with courtyard trenches and connected cellar escape geometry.
- **Silent beats:** burned block → escaped window opening → courtyard trench line → interrupted household objects and absent family positions.
- **Confidence:** High. The interview was recorded soon after the event, but by a state-organized Historical Commission.
- **Source:** Agrafena Pozdnyakova, Historical Commission interview, 14 March 1943, describing the block burned on 17 September and events of 27 September; [Russian edited extract](https://daily.afisha.ru/archive/vozduh/books/stalingradskaya-bitva-glazami-ee-ochevidcev/) and [English edited edition, chapter 18](https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php).

### A1-02 — Oil-water railway siding

- **Class / placement:** `district_event`; rail siding with cisterns, exposed bucket route, and civilian-access edge.
- **Silent beats:** empty containers at shelter → footprints crossing fire-swept ground → tanker outlet → oily sheen and contaminated buckets on the return path.
- **Confidence:** High for the practice; medium for using the catalog photograph as the same tanker.
- **Sources:** Pozdnyakova interview, 14 March 1943, [Russian extract](https://daily.afisha.ru/archive/vozduh/books/stalingradskaya-bitva-glazami-ee-ochevidcev/); Stalingrad Museum-Reserve photo inventory MZSB KP 2743/2, [catalog page](https://stalingrad1942-1943.ru/index.php/photo_list?page=1).

### A1-03 — Grain-elevator extraction convoy

- **Class / placement:** `exact_landmark`; the Stalingrad grain elevator and its service approaches.
- **Silent beats:** disturbed grain store → sacks on controlled loading route → carts driven under guard → partly opened sacks and civilian containers stopped behind the convoy.
- **Confidence:** High. Do not invent a specific exchange between named people.
- **Sources:** Pozdnyakova interview, 14 March 1943, [Russian extract](https://daily.afisha.ru/archive/vozduh/books/stalingradskaya-bitva-glazami-ee-ochevidcev/) and [English chapter](https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php); Stalingrad Museum-Reserve photo MZSB KP 1264, folio 166, [catalog page](https://stalingrad1942-1943.ru/photo_list?page=16).

### A1-04 — Horse road beside a Soviet hospital

- **Class / placement:** `attested_condition`; exposed road near a hospital or aid point, with wagon traffic and failing draft animals.
- **Silent beats:** broken traces → collapsed horse and stopped cart → evidence of civilian salvage → stripped remains farther down the same road.
- **Confidence:** High for the recurring condition. Never encode the diary’s approximate count as a literal census.
- **Sources:** Pozdnyakova interview, 14 March 1943, [Russian extract](https://daily.afisha.ru/archive/vozduh/books/stalingradskaya-bitva-glazami-ee-ochevidcev/); anonymous 79th Infantry Division diary, entries 5 and 9 November 1942, Stalingrad Museum-Reserve KP 25221, [captured translated record](https://stalingrad1942-1943.ru/node/2723).

### A1-05 — Apartment stripped into firewood

- **Class / placement:** `attested_condition`; damaged residential block with cellar shelter and deep communication trench access.
- **Silent beats:** broken room boundary → missing doors and dismantled furniture → fuel carried into shelter mouths → ash and shortened furnishings around a nearly empty interior.
- **Confidence:** High for the observed condition; the diarist is anonymous and the surviving text is a Soviet wartime translation.
- **Source:** anonymous 79th Infantry Division diary, 12 November 1942, Stalingrad Museum-Reserve KP 25221, [captured translated record](https://stalingrad1942-1943.ru/node/2723).

### A1-06 — Gumrak panic and burning stores

- **Class / placement:** `district_event`; Gumrak route and settlement/airfield support zone after encirclement.
- **Silent beats:** converging vehicle and wagon tracks → deliberately burning stores and kit → abandoned papers kept unreadable → serviceable guns and tanks standing beside the destruction.
- **Confidence:** High for the district condition; the exact hour and sequence are ambiguous.
- **Source:** anonymous 79th Infantry Division diary after encirclement, Stalingrad Museum-Reserve KP 25221, [captured translated record](https://stalingrad1942-1943.ru/node/2723).

### A1-07 — Pitomnik failed-evacuation airfield

- **Class / placement:** `district_event`; Pitomnik airfield only, with runway, dispersed bunkers/aid posts, and artillery exposure.
- **Silent beats:** cratered or lost strip → dispersed aid bunkers → interrupted movement of wounded toward the field → transport wreckage and supply containers held as attribution-qualified reference.
- **Confidence:** High for the medical/airfield condition; medium for attributing the cataloged Ju 52 photograph to Pitomnik.
- **Sources:** Army Group Don chief medical officer report, 21 January 1943, NARA T311 R271 001189–90, [reproduction](https://stalingrad1942-1943.ru/index.php/node/2654); Stalingrad Museum-Reserve photo MZSB KP 2453, [Ju 52/supply-container catalog](https://stalingrad1942-1943.ru/taxonomy/term/1499?page=1).

### A1-08 — Factory cellar ward

- **Class / placement:** `district_event`; factory/Volga belt cellar with a separate neighboring doctor position.
- **Silent beats:** dark cellar threshold → dense improvised ward → small floor fire and once-daily ration vessel → exit route toward a doctor located in another cellar.
- **Confidence:** High. The author is unnamed and the diary survives through a captured translation dated 1 January 1943.
- **Source:** German corporal, 10./Infanterie-Regiment 578, 305th Infantry Division, diary entries 22 November–18 December 1942, Stalingrad Museum-Reserve KP 25224, [captured translated record](https://stalingrad1942-1943.ru/node/2724).

### A1-09 — Sanatorium surrender ward

- **Class / placement:** `exact_landmark`; the attested sanatorium ward and its surrender approach, not a generic clinic.
- **Silent beats:** overcrowded ward and frostbite care failure → new arrivals with no treatment capacity → weapons deliberately separated and collected → a single unarmed route out.
- **Confidence:** Medium-high. Numbers in the captive-interrogation record are approximate and should not control prop counts.
- **Source:** interrogation of Major Kulle, 27 January 1943, TsAMO F. 32, Op. 11306, D. 298, fol. 218, [archival transcription](https://stalingrad1942-1943.ru/node/534).

### A1-10 — Red Army House frozen courtyard

- **Class / placement:** `exact_landmark`; the burned Red Army House complex and courtyard.
- **Silent beats:** burned complex silhouette → gnawed horse skeletons → frozen sanitation failure → frozen human remains placed from the direct observation without converting them into a body count.
- **Confidence:** High for direct observation; access was controlled and the broadcast context had propagandistic framing.
- **Sources:** Alexander Werth, BBC Home Service Russian Commentary, 9 February 1943, [transcript collection](https://www.worldradiohistory.com/BOOKSHELF-ARH/History/Voices-from-Britain-1939-1945-Krabbe-1937.pdf); [broadcast-reference verification](https://www.tandfonline.com/doi/abs/10.1080/01439685.2024.2370636).

### A1-11 — White-sheet surrender road

- **Class / placement:** `attested_condition`; late-January surrender route with poor exact geolocation.
- **Silent beats:** white sheet used as a distant nonverbal signal → prisoners with quilt- and sheet-wrapped feet → weapons deposited away from the column → unarmed movement onward.
- **Confidence:** Medium-high. It is an early-1943 commission interview, but the exact road is not securely located.
- **Source:** Captain Nikolai Aksyonov, early-1943 commission interview, [edited excerpt and photograph](https://www.sueddeutsche.de/politik/schlacht-um-stalingrad-als-hitler-den-krieg-verlor-1.1521690).

### A1-12 — Red October factory cemetery

- **Class / placement:** `exact_landmark`; Red October factory belt.
- **Silent beats:** consolidated unit positions → repeated temporary grave and marker gaps → many unburied dead → abandoned defensive works continuing through the factory terrain.
- **Confidence:** High for the condition. The diary’s estimate must not become an exact or exhaustive visual count.
- **Sources:** anonymous 79th Infantry Division diary, entries 5, 8, and 11 January 1943, Stalingrad Museum-Reserve KP 25221, [captured translated record](https://stalingrad1942-1943.ru/node/2723); Stalingrad Museum-Reserve [photo inventory](https://stalingrad1942-1943.ru/photo_list?page=16).

## Die Heimkehr — Oderbruch, Seelow, and Küstrin (internal Act II)

Act II has four non-interchangeable envelopes: the wet Oderbruch floodplain with drainage ditches and raised routes; the Seelow slope and smoky marshy bottomland; the forest staging zones around Großer Trepliner Lake; and the fortified/ruined Küstrin urban crossings. Thaw water and constrained crossings must shape traversal. Searchlights do not belong in dry upland woods; deep vehicle revetments do not belong in the Küstrin Old Town; chapel and replacement-bridge blocks remain tied to their landmarks.

### A2-01 — Floodwater-hidden wooden crossing

- **Class / placement:** `district_event`; Oder floodplain crossing during spring thaw, with shallow brown water, raised approaches, and engineer work on both banks.
- **Silent beats:** wheel tracks descend below the flood surface → the timber crossing disappears under water → generators/materials stage on both banks → the same route re-emerges beyond the channel.
- **Confidence:** High. The evidence supports accidental submersion and concealment, not deliberate camouflage.
- **Source:** Grzegorz Szalast and Grzegorz Kiarszys, “Defence in the Rhythm of Richard Wagner’s Music: The Last Days of ‘Fortress Küstrin’,” in *Sensing the Past* proceedings (2015), interpreting TARA aerial photographs dated 11 and 14 April 1945, [PDF](https://archiv.ub.uni-heidelberg.de/propylaeumdok/2513/1/SENSING-THE-PAST-Proceedings-online.pdf).

### A2-02 — Dead searchlight battery in the marsh

- **Class / placement:** `district_event`; Seelow marshy bottomland on 16 April, beside a raised route and drainage ditches.
- **Silent beats:** searchlights and generators → cable fan across mud → smoke and failed illumination → jammed infantry/armor route around the dead equipment.
- **Confidence:** High for the event; medium for exact battery dressing because the source does not locate individual searchlights.
- **Source:** Earl F. Ziemke, *Stalingrad to Berlin: The German Defeat in the East*, chapter XXI, U.S. Army Office of the Chief of Military History (1968), [online edition](https://www.ibiblio.org/hyperwar/USA/USA-EF-Defeat/USA-EF-Defeat-21.html).

### A2-03 — Civilian checkpoint at a mined bridge

- **Class / placement:** `district_event`; Küstrin Oder/Warthe bridge approach in February–March 1945.
- **Silent beats:** barrier and sentry position → non-readable personal pass shape → demolition cable/mined approach → refugee cart turned back from a damaged span.
- **Confidence:** Medium-high. Checkpoints and restricted access are supported; the final closure order/date remains unknown.
- **Source:** Küstrin Fortress Museum, “When was passage over the bridge definitively closed to civilians?”, drawing in part on Hermann Thrams’s diary-based history, [museum investigation](https://muzeum.kostrzyn.pl/de/oferta-edukacyjna/historia-w-pytaniach-i-odpowiedziach-wersja-rozwojowa/item/502-kiedy-definitywnie-zamknieto-przejazd-przez-most-na-druga-strone-odry-dla-ludnosci-cywilnej).

### A2-04 — Refugees and broken-unit stragglers through the fortress

- **Class / placement:** `district_event`; Küstrin through-route in January 1945.
- **Silent beats:** overloaded wagons and handcarts → children/elderly clustered around transport → fitter civilians walking → soldiers without coherent unit organization absorbed into the fortress flow.
- **Confidence:** High for the recurring Küstrin flow; medium for exact human arrangement. The detailed composition is partly supported by retrospective testimony from nearby Potsdam, not Küstrin itself.
- **Sources:** Küstrin Fortress Museum, “1945,” [museum history](https://muzeum.kostrzyn.pl/de/stadt-festung/1945?contrast=night); Dorothea Günther, “Das Kriegsende in Potsdam 1945,” recollection submitted 2010, [German Historical Museum testimony](https://www.dhm.de/lemo/zeitzeugen/dorothea-guenther-das-kriegsende-in-potsdam-1945).

### A2-05 — “Küstrin Pompeii” street block

- **Class / placement:** `district_event`; only within the Küstrin Old Town footprint.
- **Silent beats:** intact street line → open cellars and isolated façades → collapsed roofs/cratered courtyards → fortress masonry surviving beyond almost-total domestic loss.
- **Confidence:** High for the physical condition. Do not encode an unresolved casualty total.
- **Source:** Küstrin Fortress Museum, “1945,” using an aerial photograph dated 18 March 1945, Soviet search-film footage, and before/after imagery, [museum history](https://muzeum.kostrzyn.pl/de/stadt-festung/1945?contrast=night).

### A2-06 — Surviving brick chapel beside the municipal hospital

- **Class / placement:** `exact_landmark`; the old-Lutheran chapel and immediate municipal-hospital block.
- **Silent beats:** wholesale surrounding collapse → chapel roof still largely intact → one surviving stained-glass rose amid shattered glazing → looted interior and destroyed rear context held within evidential limits.
- **Confidence:** High for the chapel’s physical condition. The source does not securely establish every detail of the neighboring hospital’s damage.
- **Source:** Küstrin Fortress Museum chapel-history study (2021), using Luftwaffe aerial photographs of 7 February and 7 April 1945, [museum PDF](https://muzeum.kostrzyn.pl/pl/z-zycia-twierdzy/aktualnosci/item/download/32_d33a35b1375c789e57afb421c8a88e68).

### A2-07 — First train over replacement Oder bridges

- **Class / placement:** `exact_landmark`; Küstrin replacement road/rail crossing beside the fallen original East Railway spans.
- **Silent beats:** fallen original spans → new temporary construction → locomotive crossing toward Berlin → ruined city and bridge debris visible below.
- **Confidence:** High for the event; exact trestle form and train consist require a dedicated photograph before final modeling.
- **Source:** Küstrin Fortress Museum, “1945,” drawing on a 1952 Soviet General Staff tactical collection for the 25 April 1945 event, [museum history](https://muzeum.kostrzyn.pl/de/stadt-festung/1945?contrast=night).

### A2-08 — Reitweiner Sporn forward army command complex

- **Class / placement:** `exact_landmark`; surveyed Reitweiner Sporn earthwork complex.
- **Silent beats:** vehicle shelters on approach → communication trenches → varied dugout sizes → observation posts commanding the battlefield.
- **Confidence:** High. Some earthworks may pre-date Soviet use and have been reused.
- **Sources:** 64th Engineer Brigade combat journal, 9 May 1945, [Pamyat Naroda record 130750306](https://pamyat-naroda.ru/documents/view/?id=130750306); Weber, Passmore, Capps-Tunwell, and Davie, “The Battle of the Seelow Heights,” *Journal of Conflict Archaeology* 17(3), [open-access study](https://ueaeprints.uea.ac.uk/id/eprint/100820/1/Seelow_1945_JCA_OpenAccess.pdf).

### A2-09 — “Stein Stellung” road-blocking trench site

- **Class / placement:** `exact_landmark`; the surveyed trench and shelter system crossing the east–west route.
- **Silent beats:** road enters zigzag defensive line → linked firing positions and rifle pits → large gun pits → herringbone vehicle shelters behind the line.
- **Confidence:** Medium-high. The earthworks are certain; identification and exact historic extent as the named “Stein Stellung” remain provisional.
- **Source:** Weber, Passmore, Capps-Tunwell, and Davie, “The Battle of the Seelow Heights,” paired with Army Group Vistula situation maps dated 30–31 March 1945, [open-access study](https://ueaeprints.uea.ac.uk/id/eprint/100820/1/Seelow_1945_JCA_OpenAccess.pdf).

### A2-10 — Kurmark forest replenishment area

- **Class / placement:** `district_event`; forest tracks around Großer Trepliner Lake and the mapped Kurmark assembly zone.
- **Silent beats:** clustered vehicle revetments → gun/artillery/flak pits → shelter and store pits → track network tying the clusters together.
- **Confidence:** Medium-high. The motorized staging interpretation is strong, but individual pit functions and exclusive unit ownership are not all proven.
- **Source:** Weber, Passmore, Capps-Tunwell, and Davie, “The Battle of the Seelow Heights,” paired with Army Group Vistula maps from 27 February–31 March 1945 and an armor inventory dated 13 April, [open-access study](https://ueaeprints.uea.ac.uk/id/eprint/100820/1/Seelow_1945_JCA_OpenAccess.pdf).

### A2-11 — Dog-drawn casualty evacuation

- **Class / placement:** `attested_condition`; muddy Seelow battlefield route, without claiming the photograph’s unknown exact point.
- **Silent beats:** wounded person on a small wooden cart → dogs harnessed independently as draft team → reins held by accompanying soldier → route leading away from active combat.
- **Confidence:** High for the photographed arrangement; unit and destination are unknown.
- **Source:** Valery Faminsky, Seelow Heights, April 1945, Military Medical Museum assignment, archive ID Faminsky_Berlin1945_40, [archival image and caption](https://ww2abc.com/collections/battle-for-berlin-april-may-1945/dogs-transporting-the-wounded-from-the-battlefield).

### A2-12 — The only surviving tanker

- **Class / placement:** `district_event`; Seelow Heights, exact vehicle and point unknown.
- **Silent beats:** heavily bandaged tanker isolated before assembled soldiers → four absent crew positions implied only by discarded helmets/personal effects → no invented corpse tableau.
- **Confidence:** High for the photograph/caption; medium for reconstructing any surrounding memorial gesture.
- **Sources:** Valery Faminsky, Seelow Heights, April 1945, archive ID Faminsky_Berlin1945_42, [archival image and caption](https://ww2abc.com/collections/battle-for-berlin-april-may-1945/the-only-survivor-of-a-tank-crew-the-area-of-seelo); [Irish Times photo study](https://www.irishtimes.com/culture/art-and-design/visual-art/berlin-1945-valery-faminsky-s-extraordinary-photos-of-survival-and-suffering-1.4306912).

## Kameraden — Halbe and the Elbe (internal Act III)

Act III moves through village interiors, narrow forest roads, firebreaks, lakes, improvised headquarters, hospital campuses, gardens, and finally the Elbe crossing. Congestion is essential: the forest and water network repeatedly compresses military and civilian movement into a few routes. Exact structures—town hall, Forsthaus Hammer, Forsthaus Halbe, named street exits, Beelitz-Heilstätten, and Tangermünde bridge—must not become movable generic set pieces.

### A3-01 — Halbe’s nightly refugee relay

- **Class / placement:** `district_event`; Halbe school, inn, or barn used as temporary accommodation.
- **Silent beats:** straw bedding packed indoors → carts and sparse handwagons outside → one exhausted group departing at dawn → newly arrived possessions filling the same vacated space.
- **Confidence:** Medium. This is one retrospective civilian testimony.
- **Source:** Willi Haenecke testimony in Eberhard Baumgart, *Halbe 1945: Durchbruch in den Untergang* (1999), English edition *The Battle of Halbe, 1945* (2022), [publisher-access edition](https://www.perlego.com/book/3291221/the-battle-of-halbe-1945-eyewitness-accounts-from-hells-cauldron-pdf).

### A3-02 — Buried household and failed escape variants

- **Class / placement:** `district_event`; Halbe domestic properties, implemented as two separate variants.
- **Silent beats:** variant A—freshly disturbed yard over two buried crates; variant B—covered carts and empty harness, with a separate wounded-return trace and missing transport.
- **Confidence:** Medium. The testimony is retrospective.
- **Caveat:** The buried valuables and failed escape concern distinct households. Never combine them into one literal family narrative.
- **Source:** Willi Haenecke testimony in Baumgart, [publisher-access edition](https://www.perlego.com/book/3291221/the-battle-of-halbe-1945-eyewitness-accounts-from-hells-cauldron-pdf).

### A3-03 — Town-hall stable of unused Volkssturm weapons

- **Class / placement:** `exact_landmark`; Halbe’s former town hall and lower stable.
- **Silent beats:** incomplete foxholes/obstacles outside → abandoned militia kit → stored Panzerfaust tubes and small-arms crates below → no evidence of a completed defense action.
- **Confidence:** Medium-high for the event; medium for exact inventory layout.
- **Source:** Willi Haenecke testimony in Baumgart, [publisher-access edition](https://www.perlego.com/book/3291221/the-battle-of-halbe-1945-eyewitness-accounts-from-hells-cauldron-pdf).

### A3-04 — Mapless Dahme bridge bottleneck

- **Class / placement:** `exact_landmark`; the sole Dahme bridge and approaches among forest/lake constraints.
- **Silent beats:** converging armor, trucks, horses, refugee carts, and wounded → discarded field phones → one worn graphical civilian map with no readable labels → forest tracks disappearing beyond the jam.
- **Confidence:** High for geography and map shortage; medium for individual chaos details.
- **Caveat:** Do not depict alleged “Seydlitz troops”; the 2022 critical introduction reports no evidence for their operational existence.
- **Sources:** Baumgart, multiple German accounts, [publisher-access edition](https://www.perlego.com/book/3291221/the-battle-of-halbe-1945-eyewitness-accounts-from-hells-cauldron-pdf); Roger Moorhouse, 2022 introduction, [publisher preview](https://api.pageplace.de/preview/DT0400.9781784387143_A45622675/preview-9781784387143_A45622675.pdf).

### A3-05 — Forsthaus Hammer abandoned Ninth Army command post

- **Class / placement:** `exact_landmark`; Forsthaus Hammer and its forest approaches.
- **Silent beats:** staff-vehicle positions → cable routes into lodge → non-readable map/telephone work points → shell craters and abrupt abandonment.
- **Confidence:** High for location and function; interior fittings are normal command-post inference, not an inventoried source claim.
- **Source:** Gerd-Ulrich Herrmann, “Der Kessel von Halbe,” based on German documents and 1st Belorussian Front combat journals, [site study](https://www.geschichte-erinnern.de/das-jahr-1945/kessel-von-halbe/).

### A3-06 — Forsthaus Halbe immediately after battle

- **Class / placement:** `exact_landmark`; Forsthaus Halbe and its confined road/rail approach.
- **Silent beats:** battered lodge → directly photographed debris field → constrained approach geometry → damage carried outward into the breakout route.
- **Confidence:** High for location and photographic condition.
- **Caveat:** Obtain the full-resolution Red Army photograph from Archiv Uwe Klar before locking small architectural details.
- **Source:** Herrmann, “Der Kessel von Halbe,” reproducing a Red Army postbattle photograph from Archiv Uwe Klar, [site study](https://www.geschichte-erinnern.de/das-jahr-1945/kessel-von-halbe/).

### A3-07 — Twin barricade “death-lane” exits

- **Class / placement:** `exact_landmark`; two separate variants at Kirchstraße toward Teurow and Lindenstraße.
- **Silent beats:** narrow street → anti-tank barricade → churned armor/refugee approach → forest firebreak beyond.
- **Confidence:** Medium-high. Exact barricade materials and wreck positions need the cited archival image/book before final dressing.
- **Source:** Herrmann, “Der Kessel von Halbe,” events 28 April–2 May 1945, [site study](https://www.geschichte-erinnern.de/das-jahr-1945/kessel-von-halbe/).

### A3-08 — Destroyed mixed convoy in the forest

- **Class / placement:** `district_event`; narrow Spreewald/Halbe forest road, exact coordinates unknown.
- **Silent beats:** tightly packed transport → overlapping burned wrecks → road physically blocked → surviving forest corridor too narrow to bypass.
- **Confidence:** High for visible wreckage; medium for exact location. The photographer is unknown.
- **Source:** anonymous contemporary photograph, “Convoy halbe 1,” credited to Gedenkstätte Seelower Höhen, [Wikimedia Commons record](https://commons.wikimedia.org/wiki/File:Convoy_halbe_1.jpg).

### A3-09 — Civilian and military dead with loose horses in Halbe

- **Class / placement:** `district_event`; Halbe center and westward route.
- **Silent beats:** empty/broken harnesses → loose horses moving through shelling → wounded and dead obstructing street/pavement → bloodied-foot traces and discarded supplies continuing west.
- **Confidence:** Medium. The retrospective witness misidentifies Wenck’s army and overstates its strength; use only his ground-level observations.
- **Source:** anonymous veteran born in 1925, “Der Kessel von Halbe,” recollection published 22 October 2020, [Volksbund testimony](https://www.volksbund.de/en/erinnern-gedenken/erinnerungen-an-1945/beitrag/der-kessel-von-halbe).

### A3-10 — Immediate graves in gardens and shell craters

- **Class / placement:** `attested_condition`; Halbe gardens, roadside verges, forest clearings, and existing craters during the immediate postbattle period.
- **Silent beats:** damaged home/road → civilian digging route → fresh graves occupying domestic or shell-crater space → ordinary planting and movement forced around them.
- **Confidence:** High for the recurring condition.
- **Caveat:** Marker style, lime, and burial tools are not specified; do not standardize them without another source.
- **Source:** Volksbund, Halbe Forest Cemetery history, describing spring–summer 1945 burials, [cemetery record](https://kriegsgraeberstaetten.volksbund.de/en/military-cemetery/halbe).

### A3-11 — Beelitz-Heilstätten evacuation and adolescent final line

- **Class / placement:** `exact_landmark`; Beelitz-Heilstätten pavilions and forest corridor.
- **Silent beats:** hospital pavilions emptying → large mixed wounded column → very young defenders in mismatched service kit → conspicuous absence of radios, kitchens, artillery, and entrenching tools.
- **Confidence:** Medium-high. Exact ward contents and evacuation vehicle types require archive photographs.
- **Sources:** MDR, “Der letzte Kampf der Armee Wenck,” event 28 April 1945, [historical feature](https://www.mdr.de/geschichte/ns-zeit/zweiter-weltkrieg/1945/zwoelfte-armee-wenck-genscher-wehrmacht-elbe-tangermuende-100.html); Peter Rettich front diary, 24–30 April 1945, original in Militärhistorisches Museum Dresden, described by [Tagesspiegel](https://www.tagesspiegel.de/potsdam/potsdam-mittelmark/in-die-holle-geworfen-7594600.html).

### A3-12 — Tangermünde ruined-bridge surrender crossing

- **Class / placement:** `exact_landmark`; the destroyed Elbe bridge at Tangermünde and eastern approach.
- **Silent beats:** surrendered weapons and abandoned baggage held to separately sourced limits → ruined span → narrow improvised platform → long disarmed column crossing west.
- **Confidence:** High for soldiers, bridge, and platform. Civilian crowd density and weapon-pile design need separate imagery.
- **Source:** William Vandivert, LIFE Photo Collection, Tangermünde, May 1945, TimeLife image 638117, [Google Arts & Culture record](https://artsandculture.google.com/asset/jQHYx88Y5n8dUQ?hl=en).

## Research gaps before final art lock

1. Obtain full-resolution, rights-cleared reference images for Forsthaus Halbe, the Küstrin chapel, the Küstrin replacement bridges, and the Pitomnik/Gumrak aircraft records.
2. Resolve the Pitomnik Ju 52 image attribution before using aircraft placement as evidence rather than atmosphere.
3. Identify the sanatorium building and approach precisely enough to support an `exact_landmark` anchor in campaign coordinates.
4. Geolocate the Stalingrad white-sheet surrender road; until then it remains an `attested_condition`.
5. Acquire the underlying archival image/book references for Kirchstraße and Lindenstraße barricade materials and wreck placement.
6. Separate direct source transcription from modern museum interpretation for the Küstrin bridge closure and replacement-bridge construction.
7. Verify the full metadata/title page of the 2021 Küstrin chapel study.
8. Audit image and text rights before bundling any reproduction; this dossier supports factual modeling, not unlicensed asset copying.
9. Add Russian- and German-language archival review for each priority scene before final body placement, uniform detail, signage shape, or unit-specific equipment is locked.
10. Playtest whether every scene’s causal chain is readable without UI text from both a tank viewport and an on-foot route; if it is not, revise spatial composition rather than adding exposition.
11. Research vehicle and route chronology separately: identify what Tiger unit, if any, could connect any subset of these places; test travel time, front-line access, and dates; and label the campaign journey a fictional composite until that work is complete.

## Production rule

The JSON catalog is authoritative for stable IDs, priority, provenance class, compatibility tags, and machine-readable caveats. This document is authoritative for interpretation. If they disagree, stop placement work and reconcile both files in the same change.
