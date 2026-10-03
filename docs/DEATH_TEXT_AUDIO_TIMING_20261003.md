# Heavenly cue at memorial text

The fatal physical sound is unchanged. Fatal UI entry selects and decodes one finite death cue but does not submit it to the terminal mixer. The normal 1.5-second fade remains. The text deadline stays at half the selected cue's source-plus-effect-tail duration after fatal entry, matching the former intended midpoint delay. Text timing now uses the terminal presentation clock, independent of the silent, unstarted cue.

The first successfully acquired and presented memorial text frame calls the idempotent start API. Repeated frames cannot choose another cue or restart it. Missing surface frames therefore hold the cue instead of starting it before text appears. The full source and authored effect tail play from the beginning; the death screen waits for actual playback completion and then drains the existing shared master lookahead before leaving. Device buffering can add its normal audio-output latency; this is not a hardware audiovisual synchronization claim.

No fatal damage, physical propagation, world-audio fade, KIA cut, score gain, or campaign retry rule changed. A missing/undecodable cue retains the existing unscored death fallback.

Validation: the three standalone `death_card` CPU tests pass, including reveal without an audio clock, a missed first frame, paused playback, and completion withholding. The added `audio::soundscape::tests::death_preparation_is_silent_and_text_start_is_once_with_full_output_drain` test exercises the real device-free mixer, pending the coordinated integrated build. The existing `soundscape-review` death fixture now emits separate fatal/reveal/start/end receipt times and checks pre-text silence, once-only start, full cursor completion, KIA survival, bounded output and final silence. Its HTML timing captions were updated. This fixture is an explicitly staged audio graph review, not gameplay footage. The older mechanic death fixture only covers the silent replay fallback and has not been relabeled as memorial evidence.

Run after the coordinated build:

```sh
target/release/deps/panzerrechnersystem-928cf5b39904289a death_card::tests --nocapture
target/release/deps/panzerrechnersystem-928cf5b39904289a death_preparation_is_silent_and_text_start_is_once_with_full_output_drain --nocapture
KESSEL_ASSET_ROOT="$PWD" KESSEL_ENABLE_PIZZA_AUDIO=1 target/release/panzerrechnersystem soundscape-review captures/death_text_start_20261003
python3 tools/build_adaptive_audio_review.py death_text_start_20261003
```

Integrated validation completed:11death-related Rust tests passed, including the real mixer pre-text silence/idempotent start/full drain regression. The release build passed. `captures/death_text_start_20261003_receipt.json` confirms every death-route check, with six night-loop fixtures and context transitions through the final137-cue configuration. The listening page is `captures/death_text_start_20261003_adaptive_review.html`; its text timeline is a fixture display, not native death-screen footage.
