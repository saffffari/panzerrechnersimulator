# Old-renderer audio review, 2026-10-01

This isolated checkout retains renderer baseline `c6bdb335be8fc471bf6232198f67202ae4b904fe`. Audio changes are ported from the current working checkout without its source-BVH or cabin-medium renderer changes. This note does not validate the old renderer against the physical-truth release gate.

## Implemented audio changes

- The common balanced reproduction curve lifts quiet sources by at most 12 dB, tapering to zero at 110 dBA. `KESSEL_AUDIO_RANGE=reference` selects the prior reference curve before launch. Source pressure, propagation and hearing inputs are unchanged.
- Track contact sound follows actual hull travel and fourteen terrain-to-hull samples. The sample locations, 0.13 m contact cadence and contact kernels are authored approximations, not recovered wheel forces or a solved suspension. The nominal contact component sits 12 dB below the existing engine/track estimate; this is an authored component budget, not a measured Tiger energy split. No movement produces no new contact excitation; engine-off coasting can still produce contacts.
- Invalid motion samples break the travel interval. New matches reset accumulated travel/excitation. The one streaming track voice follows pause, master volume, death fade and KIA silence.
- Explicit player-interior ammunition events retain headset transfer but do not traverse their own hull as if emitted outside. Exterior wreck events keep their existing exterior path. Source provenance, rather than proximity, selects the path.
- Destruction pressure synthesis retains one compression/rebound, with a shorter coincident body component. Its modal scales are authored waveform choices, not a measured pressure history. The change reduces sub-50 Hz energy consuming playback headroom. It does not add source pressure or bypass the limiter/hearing model.

The narrow approved visual changes remove the false firing light behind the closed breech and use a compact RGB-preserving bloom kernel with portal response 0.14. All other renderer differences are owned and documented by the capture/display lane.

## Verification

`artifacts/runtime/old_renderer_audio_port_tests_20261001.log`: **96 passed, 0 failed, 3 existing diagnostic tests ignored**. Includes engine placement, enemy vehicle positioning/retirement, hydraulic traverse law, recording resolution, tracks, pause, mute and KIA gates.

`artifacts/runtime/old_renderer_audio_physical_paths_tests_20261001.log`: **12 passed, 0 failed**: reproduction 8, internal ammunition path 1, headset-filtered destruction body 1, destruction shape 1, shell pass 1. The reproduced catastrophic blast remains 9.099 dB above the small armor impact at 100 m in the calibrated hierarchy check.

The enemy-vehicle test now checks the actual common reproduction curve: a 90-to-70 dBA physical difference maps to a 13 dB difference in balanced playback and 20 dB in reference playback. The prior 20 dB digital expectation assumed the reference curve. Its original numeric tolerance is retained.

## Actual gameplay media

Baseline: `captures/old_renderer_blizzard_cupola_v1_20261001.mp4` and its ordinary live-sink soundtrack. The 16 s run passes its eight capture gates; the listener dies at 7.6667 s and later audio is silent. The opening 0.5–3.0 s mixed bed measures -51.71 dBFS unweighted RMS / -40.04 dBFS peak; the 3.5–4.0 s own-shot window measures -12.78 RMS / -1.02 peak. Mixed windows contain overlapping sources and do not measure isolated engine or enemy audibility. Baseline numeric receipt: `artifacts/runtime/old_renderer_live_audio_baseline_20261001.json`.

Updated native capture: `captures/old_renderer_blizzard_pair_v2_20261001.mp4`, its Bayer/blue-noise comparison videos and ordinary live-sink WAV. It passes all eight capture gates and records two Tiger shots, three enemy shots and KIA at 11.25 s. Its opening 0.5–3.0 s mixed bed is -45.43 dBFS RMS: 6.28 dB above baseline. The first own-discharge 500 ms window remains effectively unchanged (-12.710 baseline / -12.705 candidate dBFS RMS, both about -1.02 dBFS peak). Maximum master reduction is 10.12 dB, versus 10.35 baseline. Every PCM sample after KIA + 2.5 s is zero.

The candidate uses farther opening lanes than baseline, so the combat event sequence, enemy distances and later hit times differ. This is **not** a matched whole-clip comparison; the later impact windows cannot be compared as if they were the same event. Numeric receipt `artifacts/runtime/old_renderer_live_audio_pair_20261001.json` retains that distinction and both actual event timelines. The quiet bed also contains wind, traverse and distant vehicles, so its 6.28 dB increase is a mixed-scene observation, not an isolated engine calibration measurement.

The candidate's stationary Tiger has zero measured track travel/excitation in its opening samples; its track source therefore is not fabricating moving contacts. The physical modeled own-engine input is 83.67 dBA at the listener. The moving `old_renderer_drive_day_v2_20261001` receipt records20.515m of actual hull travel and0.2366m accumulated terrain excitation, stopping once speed reaches0 at6s. Its1.0–4.5s mixed moving bed is−25.78dBFS RMS; peak master reduction is3.23dB and all samples are finite. It records2 Tiger shots but0 enemy shots, so it passes7/8 gates and is motion/audio evidence, not a passing firefight showcase. Inside v2 passes8/8, records2/3 shots and has zero PCM after KIA+2.5s. These recordings precede the parent task's later fire/impact audio revision. Numeric details are in `artifacts/runtime/old_renderer_inside_drive_audio_20261001.json`.

Numeric checks do not establish subjective timbre or audibility approval; the actual gameplay clips must still be auditioned.

## Continuous machinery control checkpoint during live playtest

The user's movement-key popping report exposed simulation-frame gain/rate steps in engine, track, hydraulic traverse and positioned vehicle/elevation/fire loop controls. `audio_loop.rs` now applies a5ms linear gain ramp on the audio sample clock, reaches exact zero, and acknowledges silence before retiring a positioned/traverse sink. Playback-rate changes retain their phase and smooth over20ms. Procedural engine structure filters keep fixed Hz; only their existing combustion clock follows RPM. Physical source levels, calibrated steady-state gain targets, hearing and the master limiter are unchanged. One-shot attacks are outside this adapter.

The hydraulic pump's nonperiodic four-second PCM source also repeated without the existing seamless-tail fold. A100ms fold now joins that source before its actual clip is measured for calibrated playback. The original967-i16-unit loop-boundary discontinuity becomes8 units, matching the adjacent samples at that source join; the long central body remains bit-identical.

CPU receipts: `artifacts/runtime/continuous_machinery_smoothing_tests_20261001.log`3/3, `continuous_machinery_audio_regressions_20261001.log`101passed/3existing diagnostic ignores, and `continuous_machinery_final_checkpoint_20261001.log`1/1 seam regression. Tests cover phase and gain continuity, stereo-frame consistency, sample partitioning, retirement, unchanged settled RMS, live sink calibration/lifecycle and limits. The101-test run preceded only the final seam fold/metadata addition; the final seam test compiles that complete checkpoint. Four owned audio files are mirrored in the main checkout.

This checkpoint has not been auditioned or loaded into the running playtest. Confirmed ALSA underruns are a separate possible cause: the live graph still mixes DSP directly in the output callback, with the existing device buffer policy. No audio-driver settings, callback buffering, GPU jobs or running executable were changed. Numerical continuity does not establish that all audible pops are fixed.
