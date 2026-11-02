# Development review media index — 30 October 2026

Open [the standalone review page](../captures/development_review_20261030.html). It is a private local page with relative links, native video controls, category filters, image enlargement and single-video playback. No files were moved, converted, uploaded or published.

Inventory checkpoint: 30 October 2026. Four playable clips, eight still-image reviews, two explicit evidence gaps. This curates the useful latest evidence for the last one to two days; it is not a completion claim or an exhaustive media dump. The source-20 checkpoint in [the development log](EXPEDITION_DEVELOPMENT_LOG_20261030.md) is the validation authority available while this index was prepared. Newer source edits may not yet have matching captures.

## Playable clips

| Feature | Actual file | Duration / speed | Audio | Evidence scope |
|---|---|---|---|---|
| Loader handling & crew balance | [crew_loader_balance_20261030_v2.mp4](../captures/crew_loader_balance_20261030_v2.mp4) | 18 s, real time, 24 fps | Ordinary gameplay mix | Deterministic turret witness; enemy fire held |
| Physical repair manual | [expedition_manual_20261030_v11_readable_page_turn.mp4](../captures/expedition_manual_20261030_v11_readable_page_turn.mp4) | 4 s presentation, 24 fps | Silent | Staged page selection/turns; v11 pose |
| Vehicle AI: approach, hold & fire | [vehicle_brawl_fix_20261030_b.mp4](../captures/vehicle_brawl_fix_20261030_b.mp4) | 7.5 s, 90 simulated seconds at 12× | Silent | Four-vehicle staged brawl; invincible referee |
| An overturned tank stays disabled | [vehicle_inverted_fix_20261030_b.mp4](../captures/vehicle_inverted_fix_20261030_b.mp4) | 1.67 s, 20 simulated seconds at 12× | Silent | Deliberately inverted starting vehicle |

Durations, streams and encoded dimensions were read with ffprobe, not inferred from names. Loader is 1920×1080 encoded from 800×450 native; manual is 1280×720; vehicle clips are 480×270. Brawl and inverted recordings sample one frame per simulated second and play at 12 fps; they are not real-time movement footage.

## Feature catalog

### Loader handling & crew balance

- Date/revision: 30 Oct · revision 2.
- Format: Playable clip plus stills. 18 s · real-time · sound.
- Shown: One existing cartridge travels from rack to chamber. The loader crouches, carries, rams and returns to his periscope; driving and recoil move the crew.
- Limits: A staged, fixed witness camera with enemy fire held. Five ammunition checks pass; fingers, turning, obscured feet and body clipping still need work. Sampled hand targets do not prove continuous contact.
- Video: [crew_loader_balance_20261030_v2.mp4](../captures/crew_loader_balance_20261030_v2.mp4).
- Poster from the actual capture: [crew_loader_balance_20261030_v2.png](../captures/crew_loader_balance_20261030_v2.png).
- Handling the cartridge: [crew_loader_balance_20261030_v2_frame05.png](../captures/crew_loader_balance_20261030_v2_frame05.png).
- Driving and balance: [crew_loader_balance_20261030_v2_frame13.png](../captures/crew_loader_balance_20261030_v2_frame13.png).
- Recording & outcome receipt: [crew_loader_balance_20261030_v2_manifest.json](../captures/crew_loader_balance_20261030_v2_manifest.json).

### Physical repair manual

- Date/revision: 30 Oct · video v11 / stills v12.
- Format: Playable clip plus stills. 4 s · staged leaf turn · silent.
- Shown: A physical book with sixteen pages, symptom branches, diagrams and hinged page turns. Larger print receives light from the actual cabin lamps.
- Limits: The clip predates the final framing adjustment. The v12 stills below show the complete resting spread. Far-page inspection zoom is still useful; native-input readability needs a person to try it.
- Video: [expedition_manual_20261030_v11_readable_page_turn.mp4](../captures/expedition_manual_20261030_v11_readable_page_turn.mp4).
- Poster from the actual capture: [expedition_manual_20261030_v11_readable_spread_00.png](../captures/expedition_manual_20261030_v11_readable_spread_00.png).
- Latest v12: full resting spread: [expedition_manual_20261030_v12_framed_spread_00.png](../captures/expedition_manual_20261030_v12_framed_spread_00.png).
- Latest v12: inspection zoom: [expedition_manual_20261030_v12_framed_zoom_right.png](../captures/expedition_manual_20261030_v12_framed_zoom_right.png).
- Latest v12: a later diagnostic spread: [expedition_manual_20261030_v12_framed_spread_04.png](../captures/expedition_manual_20261030_v12_framed_spread_04.png).
- Video-era receipt: [expedition_manual_20261030_v11_readable.json](../captures/expedition_manual_20261030_v11_readable.json).
- Latest framing receipt: [expedition_manual_20261030_v12_framed.json](../captures/expedition_manual_20261030_v12_framed.json).

### Vehicle AI: approach, hold & fire

- Date/revision: 30 Oct · revision B.
- Format: Playable clip plus stills. 7.5 s · 90 s simulated · 12× · silent.
- Shown: Four vehicles run the actual movement and firing logic. They now stop at useful range and check visibility against the opponent they are targeting. The run records three shots, penetrations and knockouts.
- Limits: A deliberately staged brawl with an invincible referee Tiger. One frame is sampled per simulation second and played at 12 fps. This cannot show real-time aiming quality, continuous contact or every battle scenario.
- Video: [vehicle_brawl_fix_20261030_b.mp4](../captures/vehicle_brawl_fix_20261030_b.mp4).
- Poster from the actual capture: [vehicle_brawl_fix_20261030_b_0000.png](../captures/vehicle_brawl_fix_20261030_b_0000.png).
- A later moment in the 90-second fixture: [vehicle_brawl_fix_20261030_b_0060.png](../captures/vehicle_brawl_fix_20261030_b_0060.png).
- Timing & vehicle-state receipt: [vehicle_brawl_fix_20261030_b.json](../captures/vehicle_brawl_fix_20261030_b.json).

### An overturned tank stays disabled

- Date/revision: 30 Oct · revision B.
- Format: Playable clip plus stills. 1.67 s · 20 s simulated · 12× · silent.
- Shown: The deliberately inverted vehicle produces no horizontal driving and no gunfire. Its source geometry stays supported above the terrain throughout the short review.
- Limits: The tank starts upside down. This tests the disabled state and roof support; it does not demonstrate a natural rollover, full tumbling or crash physics. Playback is accelerated 12×.
- Video: [vehicle_inverted_fix_20261030_b.mp4](../captures/vehicle_inverted_fix_20261030_b.mp4).
- Poster from the actual capture: [vehicle_inverted_fix_20261030_b_0000.png](../captures/vehicle_inverted_fix_20261030_b_0000.png).
- State after 19 simulated seconds: [vehicle_inverted_fix_20261030_b_0019.png](../captures/vehicle_inverted_fix_20261030_b_0019.png).
- Inverted-state receipt: [vehicle_inverted_fix_20261030_b.json](../captures/vehicle_inverted_fix_20261030_b.json).

### Board, drive, brake & leave the halftrack

- Date/revision: 30 Oct · driving review A.
- Format: Stills only. 4 stills · 8 behavior checks.
- Shown: The current person boards through the source cab contact, drives about 9 m in three seconds, uses finite fuel, brakes, and exits without changing identity or refilling ammunition.
- Limits: Starting beside the cab is staged. This is an exterior-camera, kinematic prototype with static track belts. Cargo, towing, the crane, protection, crash accuracy and save/resume are not demonstrated.
- 1 · Boarded: [halftrack_driving_20261030_a_board.png](../captures/halftrack_driving_20261030_a_board.png).
- 2 · Driving: [halftrack_driving_20261030_a_drive.png](../captures/halftrack_driving_20261030_a_drive.png).
- 3 · Braked: [halftrack_driving_20261030_a_brake.png](../captures/halftrack_driving_20261030_a_brake.png).
- 4 · Back on foot: [halftrack_driving_20261030_a_exit.png](../captures/halftrack_driving_20261030_a_exit.png).
- Eight-check driving receipt: [halftrack_driving_20261030_a.json](../captures/halftrack_driving_20261030_a.json).

### Compact camp & finite medical care

- Date/revision: 30 Oct · live camp v5.
- Format: Stills only. Actual Act One camp · staged casualty delivery.
- Shown: Terrain-following shelters, beds, cots, stock racks, fuel drums and a workshop slab are installed. Six bedside checks verify pickup, admission, retained identity and wounds, finite treatment supplies, and the medic reaching the bed.
- Limits: The incapacitating wound and delivery beside the bed are staged. Extraction and transport are not shown. Some poses are unfinished; dark stock and the crowded ammunition doorway still limit visual review.
- Medical shelter and supported stock: [expedition_live_camp_20261030_v5_medical_stock.png](../captures/expedition_live_camp_20261030_v5_medical_stock.png).
- Bedside treatment state: [expedition_live_camp_20261030_v5_treated.png](../captures/expedition_live_camp_20261030_v5_treated.png).
- Ammunition shelter: [expedition_live_camp_20261030_v5_ammunition.png](../captures/expedition_live_camp_20261030_v5_ammunition.png).
- Fuel storage: [expedition_live_camp_20261030_v5_fuel.png](../captures/expedition_live_camp_20261030_v5_fuel.png).
- Latest six-check medical receipt: [expedition_live_camp_20261030_v5.json](../captures/expedition_live_camp_20261030_v5.json).

### A more inhabited camp approach

- Date/revision: 30 Oct · live camp v5.
- Format: Stills only. 180 m approach · 146 retained props.
- Shown: Bounded clusters use existing source debris and props along the camp approach. The actual run retains 146 pieces, including 27 pushable objects, with the vehicle corridor kept clear.
- Limits: The distant view still looks sparse. This dresses one short stretch, not the whole world. Performance cost has not been profiled; substantial props use source-bounds collision rather than exact triangle contact.
- Approach: latest actual-world view: [expedition_live_camp_20261030_v5_approach.png](../captures/expedition_live_camp_20261030_v5_approach.png).
- Camp-facing view: [expedition_live_camp_20261030_v5_camp.png](../captures/expedition_live_camp_20261030_v5_camp.png).
- Placement and retention log: [../artifacts/expedition-20261030/live-camp-v5.log](../artifacts/expedition-20261030/live-camp-v5.log).

### A dead crew member remains in the cabin

- Date/revision: 30 Oct · retained KIA review B.
- Format: Stills only. Staged death · six checks.
- Shown: The same loader corpse remains after eight seconds. The review also checks that the player is not duplicated as a commander model and that a living loader remains at his post while the player moves.
- Limits: This exercises one deliberately staged death path. Poses and intersections in the cramped cabin remain unfinished; the images do not establish all evacuation or death cases.
- Loader after the staged loss: [crew_retained_kia_20261030_b_kia_02_loader_dead_from_the_walk.png](../captures/crew_retained_kia_20261030_b_kia_02_loader_dead_from_the_walk.png).
- The same body eight seconds later: [crew_retained_kia_20261030_b_kia_03_loader_dead_eight_seconds_on.png](../captures/crew_retained_kia_20261030_b_kia_03_loader_dead_eight_seconds_on.png).
- Identity, presence & persistence checks: [crew_retained_kia_20261030_b.json](../captures/crew_retained_kia_20261030_b.json).

### Continue after losing the commander

- Date/revision: 30 Oct · succession camp v2.
- Format: Stills only. Earlier camp layout · three staged losses.
- Shown: Three successive deaths and replacements happen in one retained battlefield. The Tiger and its condition persist, personal ammunition stays finite, and the roster falls from sixteen to thirteen.
- Limits: These are staged deaths and succession-state checks in the earlier camp layout. The later camp views above supersede its scenery. Complete battlefield save/resume and casualty transport are not proved.
- Before succession: [expedition_succession_camp_20261030_v2_initial.png](../captures/expedition_succession_camp_20261030_v2_initial.png).
- After the third replacement: [expedition_succession_camp_20261030_v2_third_relief.png](../captures/expedition_succession_camp_20261030_v2_third_relief.png).
- Fourteen succession checks: [expedition_succession_camp_20261030_v2.json](../captures/expedition_succession_camp_20261030_v2.json).

### Solo control depends on the living crew

- Date/revision: 30 Oct · controls review v1.
- Format: Stills only. Source-Tiger studio · ten checks.
- Shown: Cruise remains active while walking inside. A living gunner accepts remote traverse; losing the gunner or driver removes the corresponding remote control. Opening the book blocks a new cruise command.
- Limits: Casualties are deliberately staged in a source-Tiger studio. These checks establish control gates and contact queries, not whether the controls feel enjoyable or whether the full expedition loop works.
- Crew and station view: [expedition_controls_20261030_v1_crew.png](../captures/expedition_controls_20261030_v1_crew.png).
- Walking while cruise continues: [expedition_controls_20261030_v1_cruise_walk.png](../captures/expedition_controls_20261030_v1_cruise_walk.png).
- Remote traverse after losing the gunner: [expedition_controls_20261030_v1_gunner_lost.png](../captures/expedition_controls_20261030_v1_gunner_lost.png).
- Ten-check controls receipt: [expedition_controls_20261030_v1.json](../captures/expedition_controls_20261030_v1.json).

### Restoring the warm interior

- Date/revision: 30 Oct · warm calibration C.
- Format: Stills only. Latest captured calibration · further dial restoration pending.
- Shown: The existing dome fixture and cabin display response restore warmer night lighting. The paired source views below make the intended interior easier to assess.
- Limits: These are stationary look-review views. The TPZ close-up still has the dark face that was rejected; the requested orange-face restoration is newer work awaiting a fresh capture. Lamp powers are authored, not historical measurements.
- Captured revision C: loader at night: [renderer_warm_20261030_c_night_loader.png](../captures/renderer_warm_20261030_c_night_loader.png).
- Captured revision C: radio station by day: [renderer_warm_20261030_c_noon_radioman.png](../captures/renderer_warm_20261030_c_noon_radioman.png).
- Pre-restoration TPZ: dark face still present: [renderer_warm_20261030_c_dial_tpz_close.png](../captures/renderer_warm_20261030_c_dial_tpz_close.png).
- Earlier same-asset binary comparison: [renderer_review_20261030_comparison.json](../captures/renderer_review_20261030_comparison.json).

### Explosion flash gives way to dust

- Date/revision: 30 Oct · snapshot 01 → 06 comparison.
- Format: Stills only. Paired 0.30 s fixture stills.
- Shown: The later image shows the early orange fireball cooling toward dust and ejecta. Both images come from the same-day staged effects comparison at eighteen simulation frames.
- Limits: This submits blast/ejecta sources directly, with no incoming projectile, armor resolution or impact sound. Other changes occurred between snapshots; the pair does not isolate one change or prove better performance.
- Earlier snapshot 01 · 0.30 s: [expedition_effects_20261030_01_030s.png](../captures/expedition_effects_20261030_01_030s.png).
- Later snapshot 06 · 0.30 s: [expedition_effects_20261030_06_030s.png](../captures/expedition_effects_20261030_06_030s.png).
- Earlier capture log: [../artifacts/expedition-20261030/effects-01-030s.log](../artifacts/expedition-20261030/effects-01-030s.log).
- Later capture log: [../artifacts/expedition-20261030/effects-06-030s.log](../artifacts/expedition-20261030/effects-06-030s.log).

### Crew-status panel & lamp pictograms

- Date/revision: In progress · no captured result.
- Format: No matching visual evidence. New review clip pending.
- Shown: The requested physical role symbols and crew-status lamps are being integrated.
- Limits: No current screenshot or clip is available yet. This card is a placeholder, not evidence that the new panel is complete, readable or working in the game.



### Monochrome navigation map

- Date/revision: Source work · new visual evidence missing.
- Format: No matching visual evidence. Capture still needed.
- Shown: The development log records roads and building outlines drawn from the actual world data, with monochrome markings.
- Limits: No matching new monochrome-map image or clip was found in the review inventory. Older October 27–28 map captures are intentionally excluded; full navigation and readability remain unreviewed.




## Required additions / missing evidence

1. New physical crew-status panel and lamp pictograms: no capture yet. The placeholder must stay pending until an actual review image or recording is inspected.
2. Orange TPZ face restoration: source work is newer than revision C. Do not replace the caption with “restored” until a new capture verifies the intended face and illumination.
3. New monochrome map: no matching capture located. Older `captures/screenshots/missions-missions_*20261027/20261028*` maps are not evidence of the new work.
4. Halftrack driving, camp/medical care, approach dressing, succession, controls and retained corpses currently have stills, not playable clips. Do not make slideshows and call them gameplay recordings.
5. A final v12 manual-framing video would supersede the v11 clip; retain the distinction until it exists.
6. Real-time vehicle AI/rollover recordings would allow movement-quality review. Existing 12× fixtures demonstrate bounded state behavior, not real-time feel.
7. The complete repair/recovery/upgrade/mission loop, live battlefield save/resume and human fun/pacing review remain unfinished or unrecorded.

## Curation and validation

- Media paths are relative to the standalone HTML in captures/. The captures-only private server intentionally exposes no source/assets. Sibling documentation and log paths appear as plain text in the gallery, not HTTP hyperlinks; this durable Markdown index retains repository links.
- All included file references are checked for existence. Native controls use `preload="none"`; posters are actual captured frames. No inline screenshot is presented as an animation.
- Excluded: superseded v1 loader exports, duplicate `_video_only.mp4`, failed/obsolete manual v2/v3/v5 clips, vehicle revision A, older unrelated video reels, raw asset previews and broad capture-folder mirrors.
- Existing `captures/videos/` reels were inventoried but not promoted as new October 29–30 feature evidence merely because their copy timestamp is recent.
- Gallery functionality still needs a browser check by the parent: playback, filters, enlarged-image keyboard focus, mobile layout. No game/GPU/Cargo run was performed while creating this page.

The larger [implementation roadmap](EXPEDITION_IMPLEMENTATION_ROADMAP_20261030.md) remains unfinished.

