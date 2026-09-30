# Physical extinguisher review — 2026-09-29

The new player-view pair exercises the mounted extinguisher with ordinary LMB pickup, release, and a fresh LMB spray press. The only new gameplay-facing code is a gaze-only review target for the existing mounted extinguisher. Neither case forces equipped state, deposits, suppression, immunity, ammunition removal, or a final outcome.

Both final fixtures seed the same disclosed contact after the recorder warmup: a hull-frame ray from `[0.5, 1.43, -0.2]` to `[0.5, 1.15, -0.2]`, requested strength `0.1` (the fire model's existing minimum is `0.16`). The actual hit is on source frame 7, the articulated basket disc. They retain 65 rack rounds plus the live chambered round. Enemy absence isolates the mechanical comparison; this is an authored experiment, not a historical incident or a combustion validation.

## Actual results

| Current input case | At 3 seconds | At 4 seconds | Outcome |
| --- | --- | --- | --- |
| Aimed, focused initial contact then gentle sweep | Fire 0; 0 live patches; initial-source coverage 0.709; 5,317 deposits | Fire 0; coverage 0.828; 7,442 deposits | Cartridge 0 had already ignited; ordinary cookoff at 4.292 s. Survival acceptance **fails**. |
| Wrong aim, about 0.594 radians away from the fire | Fire 1.4; 9 live patches; initial-source coverage 0; 4,870 deposits | Fire remains 1.4; 5,459 deposits persist; airborne grains fall to 0 after release | Wrong-aim causal checks **pass**. Ordinary cookoff follows at 4.275 s. |

The aimed acceptance still requires survival, no rack loss, extinction, and settled foam. Its red receipt is retained. A disappearing visible flame does not establish successful emergency suppression. The wrong-aim rack check is at 3 s, before the ordinary cookoff; it verifies that physical tool pickup did not consume a cartridge, not that a later ammunition explosion must preserve inventory.

Simulation receipts:

- `captures/astra/fire_aimed_lmb_20260929_eval_v3_manifest.json`
- `captures/astra/fire_wrong_aim_lmb_20260929_eval_v3_manifest.json`

Earlier failed inputs remain in their own receipts. Aimed v1 extinguished the source at 4 s but still cooked off. The shared center-floor experiment in aimed v2 spread across 24 contacts; coating its first contact did not suppress the other sources. It was rejected and the original paired contact restored. Wrong-aim v1 kept the nozzle too close to the empty bracket and produced only four useful deposits, so it did not establish the intended coating control. No fire laws were adjusted during these iterations.

## Why survival still fails

This is not a discarded-warmup accident: `source_fire` is seeded after the one-second warmup. Initial ammunition exposure is only 0.00178. The physical pickup is accepted at 0.15 s and the aimed spray begins at 0.43 s. At 1 s, deposited first-contact coverage is 0.105 and ammunition exposure is 0.444. Cartridge ignition occurs about 1.292 s after the new source starts. At 2 s, surrounding exposure has fallen to 0.323, but ignition is already irreversible in the current model.

`surface_fire::scan_ammunition` selects the strongest visible source-to-cartridge exposure, bounded by 2.5 m, using `min(intensity * 1.35 / (1 + 2 * distance²), 1.4)`. The smoothed dimensionless value has a 1/1.8 s time constant. `AmmunitionFire` immediately ignites the identified live cartridge when that value reaches 0.65, then counts down a fixed 3 s independently of later surface suppression. This behavior is explicit and tested. These coefficients are authored gameplay estimates; they do not constitute measured case temperature, propellant ignition delay, radiative heat flux, or Tiger ammunition cookoff data.

Foam coverage reduces the contacted fire sources, which correctly reduces nearby exposure in the aimed case. There is currently no direct foam-to-cartridge cooling state. The cabin source-triangle set contains hull, turret, basket, gun, hatches, shutters, and selected fittings, but not the live cartridge meshes. Consequently the current pair cannot test cooling a specific cartridge by coating its actual case. Once propellant is already internally burning, simply cooling a nearby floor must not arbitrarily erase that event.

The next development slice should first expose per-cartridge source attribution and accumulated pre-ignition heating, separating external case heating from internal propellant ignition. Actual cartridge contacts can then carry suppressant coating/cooling before ignition. This preserves inventory identity and contact geometry; it is not a proposal to lengthen a fuse until this fixture turns green. Physically meaningful coefficients need provenance and sensitivity bounds before claims of thermal accuracy.

## Reproduction and limits

Use the rebuilt release executable with `KESSEL_RENDERER=shader KESSEL_GRAPHICS_PRESET=high KESSEL_INTERNAL_RES=640x360 KESSEL_ENABLE_PIZZA_AUDIO=1 KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator`.

Fixtures:

- `catalog/shader_reviews/fire_suppression_aimed_lmb_20260929.v1.json`
- `catalog/shader_reviews/fire_suppression_wrong_aim_lmb_20260929.v1.json`

`mechanic-evaluate <prefix> <fixture>` produces state receipts without media; `mechanic-review <prefix> <fixture>` records the actual player camera and mixer. Both require a GPU even when no window is shown. The fixture preflight/guard test is `current_fire_pair_uses_identical_source_and_released_lmb_pickup`.

The source fire, spread, gas wash, suppression, and exposure remain bounded authored models. Extinguisher charge is still unlimited; no arbitrary capacity was introduced. Cabin flame and spray-specific sound routes remain absent, so these clips also expose that audio gap. Player-view visual review is separate from the state results above.

## Fatal audio queue defect discovered by these films

The first recorded aimed film had a peak of only -27.31 dBFS despite the cookoff flash. Source tracing found that `kill_fire_cookoff` queues `CookOff` after the ordinary update already drained delayed sounds. Native Death and the recorder then freeze gameplay permanently, so that voice never reaches the mixer. The same risk affects delayed `TurretPop` / `TurretRip` arrivals on fatal hits. This is an event-delivery defect, not justification for increasing source gain.

The implementation now shares the existing scheduled-sound drain between ordinary play and an audio-only terminal tick. Native and recorder Death states let already emitted wavefronts travel during the existing fade, update their normal ear state, and retire pending arrivals after the listener fades out. It does not run gameplay, accept input, move the frozen camera, generate new world events, bypass travel time, or change the source levels. Receipts now include pending sounds at the fatal instant and subsequent terminal arrival counts. The rebuilt player-view pair verifies delivery, as detailed below; the original quiet films remain evidence of the defect. The aimed survival acceptance remains failed independently of this audio fix.

CPU verification of the audio-delivery correction: both `terminal_audio` regressions pass (`/tmp/panzer_terminal_audio_20260929_test.log`). A delayed real CookOff voice enters the ordinary offline mixer exactly once, produces finite audible samples during the existing fade, and produces exact silence afterward. The boundary regression covers 24/60/120 Hz tick schedules, a fade-boundary hitch, and no duplicate advance at the fatal tick. The first test also verifies no queue travel during zero-delta pause and no inherited audio in a fresh retry mixer. These CPU checks are supplemented by the fixed player-view films below.

## Rebuilt player-view evidence

- [Aimed sweep, survival still fails](../captures/astra/fire_aimed_lmb_20260929_review_audiofixed.mp4) — 9 s, [receipt](../captures/astra/fire_aimed_lmb_20260929_review_audiofixed_manifest.json), [frames](../captures/astra/fire_aimed_lmb_20260929_review_audiofixed_contact.png).
- [Wrong aim, causal control passes](../captures/astra/fire_wrong_aim_lmb_20260929_review_audiofixed.mp4) — 6 s, [receipt](../captures/astra/fire_wrong_aim_lmb_20260929_review_audiofixed_manifest.json), [frames](../captures/astra/fire_wrong_aim_lmb_20260929_review_audiofixed_contact.png).

Both final receipts show exactly one CookOff waiting at the fatal instant, with 0.00524246 s of travel remaining, followed by exactly one terminal arrival and an empty queue. Each final soundtrack peaks at -1.00045 dBFS, compared with -27.31 dBFS in the quiet aimed baseline. The existing master limiter, not a new source gain, bounds the restored blast; deepest attenuation is 15.99 dB in the aimed case and 15.41 dB in the wrong-aim case. Subjective loudspeaker/headphone audition remains distinct from these signal checks.

All PCM samples before each fatal instant are identical to the quiet baseline. The aimed soundtrack first differs at 4.29712 s; the wrong-aim soundtrack at 4.27971 s. The aimed soundtrack is exactly silent after 6 s, and the final 0.1 s of the shorter wrong-aim soundtrack is also exactly silent. Both videos are H.264, 1920×1080, 24 fps with stereo 44.1 kHz audio, from a 640×360 shader scene.

The baseline and corrected video-only MP4 files are byte-identical for each case. Their SHA-256 values are `25559007d9c9a039633e439f6402ccabcfe64e59298f873cf4195326e6070d40` (aimed) and `4c8dea2a91195492942cdebb6b549c8937d5e178a379758c3dae07e582b5b7ec` (wrong aim). Physical pickup, real nozzle contact, persistent off-target coating, fire extinction or persistence, fatal timing, and the frozen fade are visibly preserved. Only the missing emitted sound was restored.
