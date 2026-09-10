# Astra art, asset, map and environmental-storytelling audit

Audit date: 2026-09-05 UTC. Checkout: `/home/alex/panzer/tank_simulator`, starting commit
`fb89dc75fdfc3bb67812122a3be5feacaaa164c5`. This is source inspection, local inventory, retained-image
review and a bounded Pizza audio import. No GPU render or live gameplay was run for this audit.
Characters, armor acoustics and renderer performance are covered by the companion audits.

## Main finding

The foundation is present, but much of it has stopped at a catalog, prototype or editor schema.
The full historical Pizza warehouse survived the transfer. The active runtime uses a small selected
model vocabulary; its authored spatial sounds are not connected to gameplay; and the strongest
historically sourced scenes have not become playable scene blocks. Rendering more rays alone will
not fix the shortage of composed spaces, material differentiation and audible environmental causes.

This pass completed the already selected 11-cue Pizza private audio bank. It does not supply the
missing environmental sound runtime, add music, or establish acoustic/historical accuracy.

## What is actually here

The full preserved warehouse is:

`source_assets/legacy-imports/legacy-main-assets-20260902/pizza/`

It contains purchase ZIPs, `_extracted`, `_previews`, alternate formats, Blender sources and a
previous `_asset_library`. Its provenance ledger is
`source_assets/legacy-imports/ASSET_SHA256SUMS`; the preservation purpose is recorded in
`source_assets/legacy-imports/README.md:3`. This is distinct from the selected active tree at
`assets/pizza/_extracted`.

| Model pack | Preserved warehouse originals | Active GLBs | Cataloged runtime GLBs |
|---|---:|---:|---:|
| PSX Mega Pack 3.1.3 | 489 | 489 | 28 |
| PSX Mega Pack II v1.8 | 549 | 549 | 64 |
| PSX Nature v1.7.3 | 98 | 49 | 20 |
| PSX Tech v1.2.3 | 63 | 63 | 0 |
| Modular Retro FPS Kit v1.5 | 44 | 44 | 8 |
| Total | **1,243** | **1,194** | **120** |

The warehouse additionally has 49 generated Nature `_orig.glb` preservation copies: 1,292 files is
not 1,292 unique vendor models. The active tree has the separated-branches Nature variant; the
standard variant remains in the preserved warehouse. Nothing in these counts establishes
ownership of every product the vendor sells today; they establish this purchased historical closure.

The tracked model catalog contains 155 GLBs overall, including 35 non-Pizza hero/crew/prop models.
All 155 source paths exist in this checkout. The 120 Pizza entries are 9.65% of the preserved
model count, or 10.05% of the active model count. This is a catalog selection measurement, not
a frame-visible asset count or a goal to render every asset at once.

The three versioned act manifests reference 66 distinct source paths, of which 56 are Pizza paths.
Their raw placement counts are 13,014 / 21,867 / 37,685 for acts I / II / III respectively.
Large placement counts mainly measure repetition. Code-authored interiors/gameplay expand the
model closure beyond those manifest paths (`docs/RUNTIME_ASSET_CATALOG.md:3`).

The preserved standalone texture libraries contain 439 PSX Textures images, 932 PSX Textures II
images and 18 Experimental Textures images. These are separate from model-associated texture files
and source previews. They are not exposed by the current editor's short loose-texture list.

| Audio pack | Preserved files | Initial runtime state | State after this pass |
|---|---:|---|---|
| Echoes Audio Super Kit | 471 | No Pizza bank | Still no selected imported Echoes cues |
| Rust & Blood | 255 | 11 selected paths, all fingerprints pending | 11 existing selections imported and pinned |
| ROT Horror Audio | 188 | No mapping | No mapping; music remains unintegrated |
| Total | **914** | **0 private bank cues** | **11 private bank cues across four events** |

Counts were read directly from these directories and JSON catalogs, excluding Nature backups.
`docs/ASSET_INVENTORY.md` is explicitly a historical Unreal/Windows inventory. Its statement that
nothing is imported is not current. `docs/PIZZA_ASSET_YARD.md` correctly distinguishes 1,194/1,243
historical closures but its broad HAL path language needs to be read against the preserved source
location above. Do not rebuild, overwrite or move the recovered libraries merely to satisfy an
old document path.

## Integration gaps and their consequences

| Finding | Evidence | Consequence / next action |
|---|---|---|
| The editor cannot browse most of the preserved warehouse | `src/assetlib.rs:25` walks only `assets/pizza/_extracted`; `:26` sounds only `assets/sounds`; `:30` has a short explicit loose-texture list | Add a bounded indexed authoring source root and lazy thumbnails/waveforms; keep source assets distinct from the reviewed runtime cook. |
| City generation uses a narrow visual grammar | `src/city.rs:42`–`:63` chooses a single principal wall/door/floor family and four furniture choices; `:164` uses uniform block pitch | Build a few complete period-compatible building families and room functions before increasing density. Reuse prototypes and LODs. |
| Campaign geometry comes from authored manifests | `src/act_manifest.rs:3`–`:5`; `src/worldgen.rs:13` marks the old generator dev/test only | Changing `worldgen::generate` alone does not update the shipping campaign. Verify the actual versioned manifest and runtime consumers. |
| Sourced scene catalog has no runtime consumer | Search for `historical_scene_catalog`, `production_shortlist`, `visual_beats` finds no `src/` loader | Connect a scene ID to a concrete block, valid placement and research receipt. Do not describe the 36 catalog entries as implemented scenes. |
| Existing story overlay is geometry only | `src/worldgen.rs:1446`–`:1450`; `:1607` hard-codes a station field-hospital tableau; `:1669` a supply-drop field; `:2344` emits the additive overlay | It supplies human-scale visual dressing, but no event/source IDs, authored sound voices or enforced research relationships. Retain useful geometry while replacing unsupported tableaux deliberately. |
| Scene blocks do not carry implemented sound/light attachments | Only `starter.scene-block.v1.json` is a concrete block; `:40` calls it a template; `:43` has placeholder source IDs. `src/assetlib.rs:1273` excludes sockets/connectors/authoring metadata from placements | A portable scene block needs object, sound, light and acoustic-region transforms that remain together when moved or rotated. A named socket by itself is not a sound. |
| Editor sound placement stops before gameplay | `src/editor.rs:754` reads sound emitters and `:1032` saves them. `src/spatial_audio.rs:61` defines positions/policies; `:276` defines `tick` | No production caller of `spatial_audio::tick` or `EmitterRuntimeState` exists outside that module's tests; `game.rs`/`audio.rs` do not consume these records. The editor UI can save an emitter without making it audible in a campaign. |
| Emitter model itself is incomplete for the vision | `src/spatial_audio.rs:206` implements a radius envelope; the record has position, gain, radius, duration and policy | Integrate this with the existing world listener/occlusion/room system, bounded voices, oriented sources, save state and near/far routing. Radius attenuation alone is not spatial acoustics. |
| Model material import loses authored distinctions | `src/gltf_load.rs:266` chooses the first material for the merged mesh; `:299` substitutes rough dielectric values when no metallic/roughness texture exists | Audit the selected hero modules first. Preserve single-material Pizza atlases, but reject/split unsupported multi-material assets and support reviewed scalar material overrides. Do not globally make everything metallic to obtain reflections. |
| Catalog LOD fields are mostly hooks | All 120 Pizza entries have exactly one `lod_hook.levels` entry | Budgeting a field is not an implemented lower-detail mesh chain. Preserve shared prototypes and add actual tiers for the visible slice. |

The 3D editor schema exists and is useful. Its sound and scene-block omissions are foundational
runtime work, so spending days manually placing hundreds of sounds now would create misleading
editor-only progress.

## What the images show

I inspected existing images in place; no old media was moved or modified:

- Pizza exterior: `source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_previews/PSX Mega Pack II v1.8/Preview_1.jpg`.
- Pizza interior: `source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_previews/PSX Mega Pack 3.1.3/Preview_1 (version 2.9).jpg`.
- Pizza tool and computer prop sheets in those same preview directories.
- HAL exterior: `captures/hal4090_gate_g06_rtsun_20260903_fix2_day.png`.
- HAL interior sequence: `captures/hal4090_fx_terror_video_20260903_contact.png`.
- Older modular room: `captures/artifacts__modular-building-gallery__ground_store_raw.png`.

The Pizza scenes have readable silhouettes, textured midtones, complete architectural boundaries,
clusters of functional props and restrained local lamps. The historical era of many props is
inappropriate for Kessel: the vendor living room's television and the separate computer sheet
are visual-material references, not a WWII furniture manifest.

The retained HAL exterior has heavy high-frequency sky noise and near-black building silhouettes;
the foreground rectangular structure lacks readable material identity. The interior sequence is
predominantly bright yellow/gold, including broad surfaces, rather than cold steel with small warm
pools. The older room proves doors, stairs and masonry modules exist, but reads as a sparsely
furnished shell. These are visual judgments about those exact retained captures, not assertions
that every current camera or setting produces the same image.

The existing art bible already asks for dark steel, restrained warm accents, material readability
and embodied composition (`docs/ART_BIBLE.md:28`, `:61`). The next art pass should enforce these
agreements through two fixed cameras and one moving route before changing the default resolution
or adding more post-processing.

The [vendor's Mega Pack II page](https://pizzadoggy.itch.io/psx-mega-pack-2), checked live, explicitly
describes its exterior/interior modular role. In the creator's replies, showcase scenes are described
as Blender renders, not included playable scenes; individual Blender sources may retain useful
settings. This matters when comparing a physically expensive live renderer against a polished
offline preview. Match spatial composition and material readability, then measure lighting cost.

## Historical scenes: research is substantial, implementation is behind it

The canonical dossier is `docs/research/environmental-storytelling-report-source.md`; its companion
`assets/worldgen/scene_blocks/historical_scene_catalog.v1.json` contains **36 scenes, 12 per act,
with 12 production-shortlist flags**. `docs/ENVSTORY.md` and `docs/research/data/envstory.json` are
older idea banks, not substitutes for the later provenance catalog.

The table below recovers all 12 shortlisted scenes. The visual/source descriptions are concise
summaries of the existing dossier. **The acoustic arrangements are new production proposals, not
eyewitness descriptions of exact recordings or measured sound fields.** Except for the additional
firewood candidate below, this pass did not independently re-verify each archive.

| Candidate | Visual and spatial causal chain | Acoustic arrangement to test | Research limit |
|---|---|---|---|
| A1-01 Burned cellar household | Burned room → damaged exit → courtyard trench → interrupted domestic space | Exterior wind through the actual broken opening; sheltered threshold drops broadband exposure; loose objects only sound when moved | Pozdnyakova's March 1943 commission interview; preserve distinct September events and absent family positions. Dossier `:82`. |
| A1-03 Grain-elevator extraction convoy | Grain storage → controlled loading route → carts under guard → civilian containers excluded | Creaking loaded carts and sack handling localized along the constrained lane; occlusion by the silo mass | Landmark-specific; no invented named confrontation. Dossier `:96`. |
| A1-07 Pitomnik failed evacuation | Exposed runway → scattered aid shelters → interrupted wounded movement | Long exposed wind field contrasted with shelter interior; distant artillery only from a represented source/time scenario | Airfield only; Ju 52 photo attribution remains uncertain. Dossier `:124`. |
| A1-10 Red Army House frozen courtyard | Entrance geometry and frozen courtyard aftermath | Sparse exterior exposure, footsteps and garment/object movement; building mass changes reflections | Werth's contemporary broadcast observation; do not invent exact people or counts. Dossier `:145`. |
| A2-01 Floodwater-hidden wooden crossing | Waterline → submerged crossing → constrained approach | Water movement around piles, board flex driven by crossing load, engine reflected/occluded by banks | Aerial-photo interpretation via archaeology; submerged geometry has uncertainty. Dossier `:170`. |
| A2-05 Küstrin destroyed street block | Street grid survives as voids between isolated façade remains | Wind through successive street openings; localized settling debris on actual contact; changing open/closed reflection paths | Museum synthesis of contemporary aerial/film evidence; reconstruct the photographed phase. Dossier `:198`. |
| A2-08 Reitweiner Sporn command complex | Communication trench → shelter → observation point | Footsteps change with timber/earth, roof shelter filters weather, equipment sounds originate at physical stations | Engineer journal plus surveyed archaeology; do not add arbitrary active radio chatter. Dossier `:219`. |
| A2-11 Dog-drawn casualty evacuation | Photograph-grounded cart, patient and dog geometry with a clear evacuation lane | Wheel/runner contact and harness motion follow movement; human nonverbal sounds need separate asset review | One Faminsky photo establishes silhouette, not a continuous animation or sound sequence. Dossier `:240`. |
| A3-04 Dahme bridge bottleneck | Routes converge at one bridge between forest/lakes; map scarcity becomes physical | Engines spatially layered by actual queue positions; separate idle/load and wood/metal contact | Mixed retrospective accounts; do not invent an easy adjacent crossing or disputed troop attribution. Dossier `:280`. |
| A3-06 Forsthaus Halbe aftermath | Reconstruct photo camera/architecture/damage before adding detail | Trees shelter one side; loose damaged structure sounds only when excited; absence of activity carries meaning | Postbattle photograph; distinguish photographed objects from authored connective tissue. Dossier `:295`. |
| A3-10 Graves in gardens and shell craters | Domestic garden/war damage reused for burial | Ground/vegetation footsteps and distant living-world sounds; silence needs no horror sting | Cemetery history supports condition, not a standard marker style, lime use or tool inventory. Dossier `:324`. |
| A3-12 Tangermünde ruined-bridge crossing | Ruined span → narrow improvised route → disarmed westward column | River beneath/alongside listener, footfall/contact on the actual platform, local baggage handling | Vandivert photograph constrains bridge/column; exact surrendered objects need their own source. Dossier `:339`. |

A particularly useful first build is **A1-05, Apartment stripped into firewood** (not one of the old
12 shortlist flags). Its prop demands fit the Pizza library and it tells a clear story without
needing a crowd, new animal rig or landmark-scale bridge. I freshly checked the
[museum's captured diary record, KP 25221](https://stalingrad1942-1943.ru/node/2723): the 12 November
entry describes damaged household property, communication trenches and surviving furniture used
as fuel. Authorship is anonymous and the surviving text is a wartime Soviet translation; it does
not establish an exact room, stove design or furniture count. The proposed close sounds—wood
contact, footsteps, wind through a missing door—are physically motivated interpretations.

The journey remains a fictional composite. The dossier explicitly says its source truth does not
validate the Tiger's presence, its route between scenes, scene adjacency or the compressed
three-phase chronology (`:17`–`:19`). Preserve that boundary in production data. A collection of
well-sourced scenes is not evidence that one Tiger crew witnessed them all.

## Proposed first playable slice

Build **one approximately 150 × 150 m ruined residential approach, courtyard, cellar and return
route to the Tiger**, using the firewood condition. Treat the size as a production starting point,
not an engine limit or historical site measurement. Select a coherent Mega Pack II building
family, Mega Pack I furnishings and a small Nature/debris set. Tag every chosen object for period
compatibility; only the necessary Tiger/crew hero assets sit outside that foundation.

1. Lock two reference views: tank-aperture approach and on-foot cellar threshold. Establish
   structure, terrain contact, open routes, furniture remnants, removed doors and fuel movement
   before adding surface clutter. Keep one clear cause-and-effect chain.
2. Implement one game-consumed scene block with a research ID and transformed geometry, emitters,
   acoustic regions and lights. Save/reload it, rotate it and enter it in the campaign. A block
   shown only in Blender or the editor fails this gate.
3. Calibrate grey/painted steel, brick, wood, brass and glass under a fixed physical lamp and sky.
   Inspect clean and standard retro treatment at the same pixel grid; fix material imports and
   exposure before using dither to hide defects. No global gold interior treatment.
4. Make the threshold audible: exterior weather, opening transmission, sheltered room response,
   footsteps/object contact and a Tiger outside that remains locatable. Voice position/state
   must survive save/reload and disappear/stop when its real cause is removed.
5. Exercise a complete short loop: approach in Tiger, stop, hatch/dismount, read the physical map,
   cross the courtyard, inspect/recover a necessary item, return and react to contact. Reuse the
   existing combat/infantry work; do not trigger a mass battle over every historical tableau.
6. Deliver a short continuous video **with captured gameplay stereo**, fixed seed/build/settings
   and event receipt; include a separately labeled diagnostic replay if needed. Do not replace
   this acceptance evidence with a montage or soundtrack assembled from unrelated events.

Acceptance should ask whether the space is readable, navigable, physically consistent and
recognizable by sound while the GPU budget still leaves room for the tank policies. More prop
types become useful once they make a distinct room or event legible; a percentage-of-bundle target
does not establish art quality.

## Map graphics

The live held-map route exists (`src/main.rs` calls `ui::draw_held_map`). Its presentation is still
screen-space rectangles and glyphs: `src/ui.rs:1971` fixes the paper rectangle, `:1979` draws a
shadow, `:1987`–`:1990` draw paper/fold lines. The cartographic strokes use repeated UI squares
(`:1917`–`:1954`). An older fullscreen paused map also remains (`:1855`–`:1911`). These are
functional presentation foundations, not a finished physical paper object.

Use the actual route geometry to create period-informed road/rail/water/contour linework with
controlled line weights and recognizable landmark symbols. Preserve stale recon marks and the
absence of a live player dot. Put that map on a real folded sheet with hands, perspective, scene
lighting and paper/contact audio; keep an accessible close inspection option. The Pizza map model
may provide a material/shape study, but its map artwork is not evidence for a historical German
sheet. Test navigability from real landmarks before adding stains, handwriting or decorative wear.

## Completed in this pass: selected Pizza bank

At the start, every selected cue's `expected_bytes`/`expected_sha256` was null, and
`src/audio.rs:881`–`:890` therefore rejected a bank even if the owner enabled Pizza audio.
There was no `assets/sounds/pizza_private` directory.

The existing 11 exact selections resolve unambiguously in the preserved Rust & Blood pack:
3 pistol, 4 rifle, 2 short distant explosions and 2 long distant explosions. Every observed hash
matches the corresponding line in the pre-consolidation `ASSET_SHA256SUMS`. I pinned those
fingerprints, refreshed `PIZZA_AUDIO_SELECTION_SHA256` and used the existing cooker. No original
source file, sound choice, layer gain or protected tank/armor synthesis identity was changed.

- Selection: `catalog/pizza_audio_selection.v1.json`.
- Selection SHA-256: `2f3ad9281e1927032fd9b18be1726f2f593f842f663cf805bea92a64508c0b14`.
- Local bank: `assets/sounds/pizza_private/`, 11 OGGs, 680,765 bytes; receipt `BANK-RECEIPT.json`.
- Candidate fingerprint evidence: `artifacts/audio/astra-pizza-selected-fingerprints-20260905.json`.
- Dry selection audition: `artifacts/audio/astra-pizza-selected-audition-20260905/pizza_audio_private_audition.wav`.
- Audition cue sheet: adjacent `AUDITION_CUES.csv`; 18.85 seconds, PCM16 stereo, 44.1 kHz.
- Remaining filename candidates: `artifacts/audio/astra-pizza-next-candidates-20260905.json`:
  45 Echoes button/lever/switch matches, 36 Rust & Blood handling matches, zero matches in the
  existing Echoes ricochet filter. These need sound/period review before becoming new selections.

The waveform file is a **dry source-selection audition**, not gameplay capture or evidence of
spatial acoustics. The owner Linux launcher now defaults `KESSEL_ENABLE_PIZZA_AUDIO=1`, while
preserving an explicitly supplied value such as `0`; it rebuilds the binary as before. Direct
binary invocation still requires that opt-in. Missing/invalid banks retain procedural audio.
No ROT music, Echoes ambience or editor-authored environment sounds were integrated here.

Reproduction commands from the repository root (the cooker intentionally requires a fresh output):

```sh
python tools/pizza_audio_bank.py fingerprints --source-root source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted --output artifacts/audio/ANOTHER-PRIVATE-FINGERPRINT-REPORT.json
python tools/pizza_audio_bank.py validate-sources --source-root source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted
python tools/pizza_audio_bank.py cook --source-root source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted --output assets/sounds/pizza_private --ack-license owner-private-game-integration-only
python tools/pizza_audio_bank.py validate-bank --bank assets/sounds/pizza_private
python tools/test_pizza_audio_bank.py
```

Source validation, cooked-bank validation and all **15 Python regression tests passed**. The tests
still cover rejection of pending fingerprints using a deliberately pending fixture instead of
requiring the real manifest to remain unfinished. The parent audit owns the combined Rust build
and runtime validation; no live audible-gameplay claim is made by this report.

The bank and audition remain gitignored owner-private game-integration artifacts. Existing
integrated-game purchase policy remains intact (`docs/THIRD_PARTY_ASSETS.md:169`); this pass did
not create a public asset library or change the non-Pizza model provenance status documented in
`docs/RUNTIME_ASSET_CATALOG.md`.
