# Three authored historical environment studies — ready for executable review

The **v4 queue** is `artifacts/astra-historical-scene-studies-20260905-v4/queue.json`.
It contains three separate playable scene manifests and three 31-second ordinary first-person
walkthrough specifications. No canonical act, menu default, previous draft, or existing video was
replaced. These are authored environment studies, **not visually approved historical reconstructions**.

## Chosen evidence and its limits

| Study | Supported condition | Authoring inference and exclusions |
|---|---|---|
| `a1_apartment_firewood` — winter residential shelter | The Stalingrad Museum-Reserve's captured diary, KP 25221, describes doors and furnishings taken for fuel and communication trenches between houses. [Museum document, 12 November 1942](https://stalingrad1942-1943.ru/node/2723). | The local ruined shells, below-grade shelter, timber trail, quantity of boards, and sparse bedding are inferred spatial composition. Blizzard is a disclosed game-weather endpoint; that day's blizzard is not established. No named household, readable diary, invented stove/fire, or intact furnished home. |
| `a2_kurmark_forest_replenishment` — clearing forest | Archaeological survey and archival maps support motorized staging/replenishment around the Großer Trepliner See/Petershagen forest, with diverse earthworks and vehicle shelters. [Weber et al., pp. 27–30](https://ueaeprints.uea.ac.uk/id/eprint/100820/1/Seelow_1945_JCA_OpenAccess.pdf). | This is an inferred district study, not surveyed coordinates. The varied pits, tracks, generic small stores shelter and unmarked crates do not identify a pit's function, prove its contents, or establish exclusive Kurmark ownership. No parked proxy vehicle substitutes for a playable vehicle. |
| `a3_immediate_graves` — bright-day damaged garden | Volksbund's institutional history records immediate postbattle burials in gardens and shell craters, and temporary sites in woods and along roads. [Halbe cemetery history](https://kriegsgraeberstaetten.volksbund.de/en/military-cemetery/halbe). | The garden, damaged houses and unmarked soil disturbances are inferred. Their number is an authoring choice, not a historical total. No names, grave-marker style, lime, burial tools, human tableau or exact plot is asserted. This Halbe study is **not** a replacement for the canonical Berlin/Reichstag finale. |

The three source pages were checked on 5 September 2026. Their locators, caveats, provenance classes
and catalog identities remain authoring metadata; no explanatory text is placed in the world.
Weather progression is an artistic campaign requirement, not a claim that these three separate
historical settings shared one meteorological sequence.

## Versioned outputs

| Scene | Final manifest | Placements / distinct GLBs |
|---|---|---:|
| Winter firewood | `assets/worldgen/revisions/historical-winter-firewood-ground-v4/20260905T222408198817Z_cc608f04/manifest.v1.json` | 364 / 19 |
| Clearing forest | `assets/worldgen/revisions/historical-clearing-forest-ground-v4/20260905T222408915580Z_671d01a5/manifest.v1.json` | 368 / 19 |
| Blue-day garden | `assets/worldgen/revisions/historical-blue-day-garden-ground-v4/20260905T222409572258Z_eab00aca/manifest.v1.json` | 307 / 22 |

Each source scene is under `assets/worldgen/scene_blocks/astra_historical_studies_20260905_v4/`.
The 144 m terrain core uses a 193×193 grid at 0.75 m spacing and a 12 m stitch apron. Local roads,
pits, berms, shelter grades and garden/crater soil shapes modify the actual common heightfield.
The final cooked worlds include the apron and quiet outer margin. Their ground maps use owned
Pizza snow/grass/dirt tiles at a local 12 m repeat with linear reflectance blending; an entire
campaign ground image is not stretched over these small studies. Ground-map source hashes are
stored alongside the derived PNGs.

There are **29 distinct owned GLB files across the batch**. The runtime asset catalog currently has
156 entries; the downloaded bundle contains additional files outside that curated set. Each used
asset has an exact repository path, SHA256, byte size, active-scene transformed bounds, triangle
count, catalog ID when present, and derivation record when applicable. These receipts describe
source identity and scale, not period authenticity. Unrelated modern props and missing hero
assets were not forced into the scenes to inflate coverage.

Source modules provide floors, walls, ceilings, openings, corner structure, interior side bays,
and modest source props. Winter trees include retained conifers and actual branch/trunk-only
derivations of the supplied deciduous trees. Spring groves include source trees, stones, logs and
stumps; they avoid the road/door routes and freshly disturbed earthwork/burial footprints.
No renderer, audio, primer, AI or gameplay Rust source was changed in this lane.

## Authoring workflow used

`tools/author_historical_studies_20260905.py` uses the repository-local editor MCP's
`pizza_asset_search`, `scene_validate`, `scene_cook`, and `world_inspect` tools. Their complete
JSON-RPC requests and structured results are in `editor-mcp-transcript.json`. The existing shared
`world_authoring.publish` API then creates a second immutable revision carrying the local source
ground map. This is the same versioned scene-authoring system used by the editor; it is not a
live editor viewport session or a rendering service.

The original **22 scene blocks and 14 unresolved exact-site plans** were hashed before authoring
and rechecked afterward: all 36 are unchanged. The whole catalog still has 36 entries. This batch
adds detail to three entries; it does not complete the remaining 33, their missing hero assets,
their historical placement work, or the actors/actions needed to communicate every event.

Earlier study iterations are preserved. **Do not queue v1, v2 or v3.** CPU QA found a doubled shelter
grade subtraction, normalized foot input moving faster than assumed, and vegetation inside a
fresh vehicle pit. v3 repaired those authoring issues and retained foundation datums after road
grading. The final v4 also restores the existing modular recipe's **10 cm floor clearance above
terrain** and raises room furnishings with it. This avoids coincident floor/terrain surfaces while
retaining a normal traversable threshold; no terrain or structural geometry is removed. The
walkthroughs now require actual standing floor height at the interior check. None of the superseded passes were
presented as game footage or visually accepted.

## Ordinary first-person review

The three review JSONs are:

- `assets/review/first_person/fp_historical_winter_firewood_20260905_v4.json`
- `assets/review/first_person/fp_historical_clearing_forest_20260905_v4.json`
- `assets/review/first_person/fp_historical_blue_day_garden_20260905_v4.json`

Each stages only an on-foot start at `(0,24)`, a look toward the room, an out-of-the-way player
tank, and banished combatants. The first 12 seconds inspect the exterior with ordinary mouse
look; ordinary forward input then reaches the threshold. The forest/garden use E at 15.5 seconds
to open their actual source-hinged leaves and wait for the swing. The player then enters the
room and looks around. The physical route, architecture and field of view are retained. Expected
movement uses the current game's normalized 5.5 m/s default foot gait, rather than assuming
analog `0.5` produces half speed. There are no injected door states, teleports after setup,
postprocessed stills, outcomes, cinematic cameras or synthetic soundtracks.

The firewood entry deliberately has missing doors. A byte-identical copy of the source wide frame
has the explicit identity `stripped_apartment_open_frame.glb`, which avoids the runtime's exact-name
automatic door-leaf installation. Original triangle/material bytes and the original GLB remain
unchanged. Other wide frames retain the ordinary closed defaults and E interaction.

Requested capture is **640×360 logical pixels, exact 3× presentation to 1920×1080, with live game
audio**, under `captures/astra/`. Root coordinates the GPU; this authoring lane ran no GPU or Cargo
jobs. The frozen v16 binary's `mechanic-validate` command returned successfully for all three specs
before renderer allocation. That is a format/manifest preflight, not a rendered appearance pass.

## Existing source work versus remaining proof

| Earlier request | Source state already present | What this batch adds / what still needs proof |
|---|---|---|
| Scene editor/MCP and versioned worlds | Native F7 scene capture, terrain stamping, the eight-tool authoring MCP and immutable revision APIs exist. | Three detailed scene blocks cooked through those APIs. Live editor placement/visual review remains separate. |
| Snow environment must contain trees | Actual bare-tree derivation preserves source branches/trunks; conifers remain leafy. | Winter study contains sparse mixed silhouettes. Final density, snow appearance and sky visibility require in-game review. |
| 3D grass, wind and tracks | `grass.rs` / `grass_filament.rs` already implement solid tapered stalks, damped bending and persistent tread contact. This is a bounded approximation, not a full strand-collision/hair solver; grass is excluded from secondary ray visibility/shadows. | The spring studies add appropriate source ground/litter, rocks, logs and groves. Grass appearance, crushing and lighting are not approved by scene validation. |
| One-pixel white snow and translucent 3D dust | `pixel_particles` retains world positions, explicit native-pixel presentation, white exposed snow, roof/upstream exclusion, and lower-opacity lit dust. Continuous snow/aerosol transport remains separate. | Existing presentation is used unchanged. No new particle or sky fix is claimed here. |
| Blizzard → clearing → bright blue day | Existing `CampaignAtmosphereProfile` endpoints set overcast/storm to `.99/.98`, `.45/.12`, and `.10/0`; physical haze/cloud coefficients vary with them. `advance_campaign_weather` smooths campaign encounter/search progress with a 45 s response. | The three review worlds use the actual **daytime endpoints** through `Game::new`. Standalone Playtest clears campaign progress, so these clips do **not** demonstrate one continuous transition. A genuine ordinary campaign progression video is still required. |
| Real enterable structures and interiors | Source modules, physical doors, common terrain and standing walk-surface support already exist. | Eight source-sized entrances/foundation datums, populated side bays and a below-grade winter shelter. Live capsule contact and door-swing behavior still need the queued footage. |
| Historical events fully built and integrated | Catalog, 22 spatial drafts and 14 unresolved plans exist. | Three local environment studies only. Human action, exact-site reconstructions, missing specific vehicles/animals, the remaining catalog and canonical campaign integration are unfinished. |

## Verification receipts and acceptance boundary

The v4 package includes exact asset provenance, immutable revision records, editor-MCP transcript,
three static route audits, shared foundation checks, ordinary-input expectations, lower-shelter
grade samples, original-draft preservation and frozen-binary preflight results. The static main
routes use source triangle rays at three body-width samples and four heights, not just a centre
ray. All three are clear; these rays do not replace the actual capsule/door simulation. All eight
entrances retain the intended 10 cm floor-above-grade step within 2 mm. The sampled shelter
approach has a maximum grade of 0.137 m/m, reaches its −1.7 m foundation grade, and meets the
−1.6 m floor with the same 10 cm step rather than a sudden drop. All used GLB SHA256 values
match their receipts.

`source-layout-plans.png` is explicitly a **CPU source plan, not a game capture**. It supports
layout review only. Actual footage must still show readable outdoor colour, coherent blue sky and
haze, correct one-pixel snow, stable lighting, believable architectural scale, source textures,
working doors and unobstructed standing movement. A test counter, a successful cook, or the map
plan alone cannot mark any scene visually approved.
