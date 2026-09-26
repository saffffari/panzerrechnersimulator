# Historical scene source audit — 25 September 2026

Prepared by Athena, AI research collaborator. Scope: the first three independently authored Act I workshop scenes. This is a source audit and assembly guide, not a survey, an exact reconstruction, or evidence that the player’s Tiger visited these places.

The existing dossier and catalog were checked against their linked testimony and archival transcriptions. Their production directives were treated as reference material; this audit does not change the user’s requested workflow, campaign, maps, or code. The older `docs/teams/historical-map-grounding.md` proposes a separate relief/withdrawal route and Tiger chronology. It cannot establish the location or date of these city and Gumrak scenes. Vehicle and campaign continuity remain a separate research question.

## Selected buildable subjects

| Catalog ID | Production subject | Historical envelope | Assembly boundary |
|---|---|---|---|
| `a1_burned_cellar_household` | Burned residential shell, shelter cellar, courtyard trench | Solnechnaya Street; September 1942 episode | A winter presentation is a later imagined survival of the ruins, not the witnessed moment. |
| `a1_factory_cellar_ward` | Dark industrial cellar used by soldiers and wounded | Stalingrad factory/Volga district; 15–18 December 1942 | No identified factory, measured floor plan, or established neighboring doctor’s address. |
| `a1_gumrak_burning_stores` | Burned supply surroundings and disrupted road traffic | Gumrak and a separately described approach from Karpovka; late November 1942 | A compact block combining both observations is a declared route composite. |

These subjects can start with existing modular walls, openings, floors, rubble, timber and containers. That is an authoring recommendation, not a completed asset inventory. Asset identity and collision suitability are being checked separately by the scene authors. Specialized aircraft, exact landmarks and distinctive medical equipment need their own verified assets; ordinary boxes cannot silently stand in for those objects.

## A1-01 — Burned cellar household

**Checked evidence.** Agrafena Pozdnyakova’s interview is dated 14 March 1943. Her account identifies a small two-storey house on Solnechnaya Street, cellar shelter, courtyard trenches, a house/block fire on 17 September, escape through house windows, surviving walls and subsequent trench shelter. The 27 September bombardment and later move to another household’s cellar are separate events. [Edited Russian primary testimony, published by Afisha from Hellbeck’s collection, 6 May 2015](https://daily.afisha.ru/archive/vozduh/books/stalingradskaya-bitva-glazami-ee-ochevidcev/).

**Source quality.** This is a named near-contemporary witness in a Soviet commission interview, accessed through an edited publication, not an original transcript facsimile. The [publisher’s collection record](https://www.nlobooks.ru/books/biblioteka_zhurnala_neprikosnovennyy_zapas/19217/) confirms the documentary edition. [Hellbeck’s account of the commission, hosted by the publisher](https://www.nlobooks.ru/books/biblioteka_zhurnala_neprikosnovennyy_zapas/19217/review/8432/), explains its wartime interviewing and political context. These establish provenance, not independent corroboration of the household’s geometry.

**Do:** make the shell, window-sized exit, cellar and courtyard shelter legible as distinct spaces. Tag any chosen stair, footprint, window height, trench alignment or snow accumulation as a reconstruction choice. Use old char, weathering and later snow if presenting winter remains. Keep the design explorable without claiming a surveyed address.

**Do not:** call the exit a documented cellar window, assert an underground connection, or freeze a September escape into an attested winter tableau. The checked episode does not inventory an abandoned table setting, toys, portraits, shoes, stove or named furniture. Such props need an inference label or their own source; do not assign them to named family members. Do not merge the later household’s cellar or deaths into an invented pose.

**Suggested developer label:** “Solnechnaya Street testimony-inspired residential ruins; September 1942 event anchor; fictional later winter dressing and dimensions.”

## A1-08 — Factory cellar ward

**Checked evidence.** The captured diary of an unnamed corporal, 10./IR 578, 305th Infantry Division, is cataloged as Museum-Reserve KP 25224. On 15 December he enters a factory cellar; on 16 December wounded descend into its darkness and a fire burns directly on the floor. On 18 December a wounded man is carried to a doctor found after searching, in another ruined house’s cellar. The later 50-metre Volga reference describes the diarist’s trench. [Primary document transcription, entries 15–18 December](https://stalingrad1942-1943.ru/node/2724).

**Source quality.** The record explicitly presents a translation of captured material received on 1 January 1943; that date is not the ward event date. The hosting documentary project identifies the Stalingrad Battle Museum-Reserve as repository. Original handwriting and translation accuracy were not independently authenticated in this audit.

**Do:** make a dark, accessible industrial shelter with restrained floor-fire residue or a small controlled fire, interrupted occupation, and a second shelter destination. Keep its connecting path an authored navigation solution. Bedding, stretcher types, bowls, fabric, lamps and partition construction are inferred assets unless separately sourced.

**Do not:** fix the factory cellar exactly 50 metres from the river, name a factory without evidence, or call the doctor immediately adjacent. The checked passage does not establish a formal hospital floor plan, bed count, single communal ration vessel, particular bedding material or surgical theatre. An empty shelter is an artistic aftermath choice, not a claim that the witness found it empty.

**Suggested developer label:** “Factory-district cellar shelter, inspired by KP 25224, 15–18 December 1942; inferred architecture and medical props.”

## A1-06 — Gumrak burning stores

**Checked evidence.** Museum-Reserve KP 25221 is an anonymous 79th Infantry Division officer’s diary preserved in translation. Within the long entry headed 22 November, the return journey reaches Gumrak the next morning: traffic arrives from Stalingrad, and houses, food and clothing burn. A separate observation on the road from Karpovka describes units burning papers/clothing while guns, tanks and machine guns remain ready. The next dated entry is 25 November. [Primary document transcription, 22–25 November sequence](https://stalingrad1942-1943.ru/node/2723).

**Source quality.** This is a wartime witness account with rumor and reported orders mixed into observation. “Late November 1942” is safer than a precise hour or day for this compound scene. The page’s title and diary opening also disagree about its starting month; neither affects the explicitly headed November entries. The hosting project names KP 25221 and the museum repository; this audit checked transcription, not the manuscript.

**Do:** retain a road/logistics character. Burned storage surroundings, sparse surviving containers and layered traffic traces can convey aftermath; their arrangement, fire timing and exact contents are reconstruction choices. If including the Karpovka cue, make it a separate roadside cluster and identify the whole block as a route composite in metadata.

**Do not:** claim all Gumrak fires were deliberate, combine both observations into one documented yard, or infer that every ready weapon was wrecked. Do not attach an exact tank model/unit, runway, aircraft dispersal or airfield building plan to this passage. A burned building need not be relabeled an ammunition depot. Pitomnik is a different place.

**Suggested developer label:** “Gumrak/logistics-route aftermath, late November 1942; compact composite of separate diary observations; inferred layout and elapsed time.”

## Corrections to carry into scene metadata

| Existing catalog or dossier shorthand | Audited treatment |
|---|---|
| Burned household: “connected cellar escape geometry” | Keep cellar shelter, house-window escape and courtyard trench distinct. Connectivity is authored, not verified. |
| Household in mandatory winter context | Separate September event date from later winter dressing. |
| Factory: “dense improvised bedding” / “single ration vessel” | Mark those prop choices inferred; preserve only the supported shelter/occupation/fire relationship as the anchor. |
| Doctor in “neighboring cellar” | Another ruined house’s cellar, reached after searching; distance unknown. |
| Gumrak: “deliberately burning stores” beside ready tanks | Separate Gumrak fires from the Karpovka-road deliberate burning and ready weapons. |
| Confidence “high” | Confidence in a reported condition is distinct from certainty about its coordinates, date granularity, dimensions and appearance. |

## Common authoring recommendation

Store `event_date_or_window`, `depicted_date_or_window`, `place_envelope`, `source_url`, `documented_relationships`, `inferred_geometry_and_props`, and `omitted_due_to_missing_assets` with each workshop block. These are suggested descriptive fields, not a required schema migration. Record unbuilt cues honestly and keep the three blocks independently placeable within their respective geographic envelopes. Their adjacency in an editor or eventual generated map is not historical evidence.

No new atrocity tableau, named victim likeness, exact casualty count, readable pseudo-document or authenticated street plan follows from this audit. No archive image was copied into game assets. The three source anchors support restrained, explorable material arrangements; they do not make an invented arrangement a historical photograph.

**Verification receipt:** linked primary transcriptions, edited witness excerpt, publisher edition record and publisher-hosted editor account opened on 25 September 2026. Original manuscript/facsimile collation, exact-site survey, source-image licensing and vehicle/route validation remain outside this bounded pass. Only this audit document was authored; maps and runtime code were not changed.
