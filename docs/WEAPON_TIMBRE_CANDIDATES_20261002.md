# Weapon timbre review candidates — 2 October 2026

The ae204a0 weapon sound was technically calibrated but rejected in play. These candidates are new authored source timbres for listening review. Meter results do not establish that they sound good or match the supplied reference.

## Main gun

The rejected source's five fixed, slowly decaying modes could make a pitched, synthetic body. The new default working candidate replaces them with a finite, non-periodic pressure response: a preserved initial pressure front, bounded low/mid excitation, a short upper edge, and a decay ending by 230 ms. A causal 90 Hz filter removes excessive deep displacement from the original impulse; the new audible low body remains. This is authored synthesis, not a measured Tiger response.

Three candidates use the same actual Audio direct-slot playback, physical level, hearing/reflex and master:

- `previous_modal`: the rejected source, retained only in the test build as an honest baseline.
- `pressure_body`: 100 ms decay with a finite final fade; working recommendation for the requested mass and attack, pending listening.
- `compact_pressure`: 55 ms decay; a shorter alternative.

Every candidate is exported buttoned and head-out. The fixture uses a fresh listener, fires at 0.25 seconds and lasts three seconds. It isolates the report; actual recoil, engine and reload mechanisms are excluded and clearly labeled in its receipt. Candidate PCM is inserted into the generated Clip slot before calling `play_at_ear`, retaining the same i16 storage/f32 playback conversion. No film recording is used.

## Machine gun

The former source applies two 155 Hz high-pass poles after its small 263 Hz shoulder. The new MG-only route keeps a 70 Hz DC block, adds a short 176 Hz body and a non-periodic upper crack limited to 4.5 ms. Rifle and P08 source generation are bit-identical to the prior profiles. The separate local receiver now has a finite mount displacement and a short upper contact edge, with the first contact finished before the feed event. No room tail is added to the local receiver. Exterior muzzle propagation still applies armor, headset and its environment response once.

Three candidates are exported: `previous`, `body_and_crack` and `sharper`. The latter uses a 90 Hz DC block and a stronger short edge. At equal A-weighted loudness, all twelve generated `body_and_crack` variants increase both 80–350 Hz body and 2.5–8 kHz edge over the previous source; the finite-tail/causality assertions also pass in standalone source checks. These measurements are for the source, not a claim about perceived audibility after every propagation path.

The new fixture is **24 actual per-round report/receiver events at 800 RPM**, not the legacy repeating MG loop used by `impact-mix-review`. Each round has one immediate receiver event and one exterior muzzle event delayed by the winter speed of sound over a declared fixed two-metre path. The ordinary sample-clock timeline, physical source levels, buttoned hull/headset transfer, hearing model and master remain enabled. This is a controlled listening geometry, not a claim that two metres is an exact authored coax locator.

The MG source may retain the early crop of the already owned/verified Pizza Doggy generic rifle recording, as the existing source bank does. This is not a verified MG34 field recording. The receipt says whether that crop was actually loaded. No additional recorded source was admitted.

## Reference limits

Original `/home/alex/Recordings/` WAVs remain reference-only. Existing PCM analysis reports that `mg_inside.wav` contains 1,171 full-scale samples and its whole-clip energy is dominated below 80 Hz; `main_gun_and_reload_perfect.wav` has 17 full-scale samples and mixes the shot with a complete mechanism sequence. Those mixed, clipped recordings cannot be treated as isolated calibrated source spectra. No reference audio was imported into runtime assets. The user's Splice rights statement applies to `/home/alex/panzer/panzer_sounds`; this weapon change does not use it.

## Validation and export

No physical SPL, global exposure, master level, propagation timing or full-mix acceptance ranges were changed. The source-only hiss check keeps its numerical limit but measures after the initial 15 ms crack, using a small analysis-boundary taper; this distinguishes the desired attack from a sustained high-frequency tail. The old total-event ratio conflated those two features. Other headroom, body, finite-tail and calibrated ordering assertions remain.

Source-only standalone checks are recorded in:

- `artifacts/runtime/weapon_gun_source_assertions_20261002.log`
- `artifacts/runtime/weapon_mg_source_assertions_20261002.log`

Root owns the integrated build. After it completes, use the actual test binary with these filters:

- `audio::small_arms::tests`
- `mg_receiver_is_one_short_varied_mechanical_cycle_without_a_firing_loop`
- `own_gun_pressure_body_preserves_transient_and_calibrated_headroom`
- `own_gun_active_variants_match_the_calibrated_float_chain_and_master`

Set `KESSEL_ASSET_ROOT` to this checkout and `KESSEL_ENABLE_PIZZA_AUDIO=1`. For the nine WAVs and route receipt, also set `KESSEL_WEAPON_TIMBRE_REVIEW_PREFIX=captures/adaptive_weapon_candidates_20261002` and run `weapon_timbre_candidates_through_live_audio_review --nocapture`. Existing output paths are refused. This export is CPU-only and opens no sound device. Until that integrated export runs, no candidate listening artifact is claimed delivered.

## Listening page

After the nine-WAV receipt is complete, run `python3 tools/build_weapon_timbre_review.py adaptive_weapon_candidates_20261002`. The builder accepts that prefix or a receipt path, validates the exact nine routes and complete 44.1 kHz stereo PCM payloads, and records file hashes. Its local page has one audio player active at a time, no autoplay and no loudness normalization. Every take starts with fresh hearing, then uses the normal changing middle-ear reflex and hearing state; it is not a fixed healthy-hearing bypass.

## Completed integrated checkpoint

The four focused filters passed nine tests total; the actual candidate exporter then passed one additional test. All nine exported files are complete three-second, 44.1 kHz stereo PCM16 WAVs, with zero full-scale PCM samples. The generated local review is `captures/adaptive_weapon_candidates_20261002_weapon_review.html`; its receipt and validation JSON accompany the files. No device playback or subjective audition was performed.

All buttoned main-gun and MG fixtures triggered zero master reduction. The head-out maximum reduction was 5.24 dB for the rejected modal baseline, 3.74 dB for pressure body and 4.46 dB for compact pressure. These are limiter measurements, not perceived quality scores.

The completed MG graph exposes a useful limit: compared with the previous per-round source, body-and-crack output has only 0.32 dB more energy at 80–350 Hz, while its 2.5–8 kHz energy is 5.92 dB higher. Thus the upper crack survives the physical route, but a substantial perceived bass improvement is not established. The sharper candidate is close to body-and-crack after the hull/headset transfer. Preserve those transfer functions when judging the next source revision.

The next useful full-mix review should use actual per-round MG on the normal running engine, then a real main-gun event with recoil/reload/case contacts and delayed impact through the same sample-clock Audio graph. The old `impact-mix-review` remains useful for its 54 signal/ordering gates, but its `mg34_burst_buttoned` case calls the obsolete repeating loop and must not be presented as proof of the new per-round MG.
