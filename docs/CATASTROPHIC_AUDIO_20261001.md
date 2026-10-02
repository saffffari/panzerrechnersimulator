# Catastrophic blast and crew microphone review

The blizzard observer capture previously collapsed every destruction into the ordinary vehicle
kill voice and its 0.71 kg TNT-equivalent estimate. The live game already distinguishes an
ammunition detonation: `TurretPop` at the existing 3.67 kg equivalent plus a separate structural
`TurretRip` layer. The recorder now preserves the authoritative destruction classification and
uses those same voices and emissions. Neither equivalent is a measured ammunition-rack yield;
their existing estimate status in `sound_levels.rs` remains unchanged.

The fix preserves the global reproduction curve, own-gun calibration, cannon bass and true-peak
limiter. Increasing every explosion's gain against the limiter would discard more of the
source difference. Equal transient peaks also do not imply equal loudness: compare event
energy and short-term A-weighted level after the master stage.

Before the classification fix, the isolated shared-path diagnostic at 22 m measured the ordinary
armor strike at -33.38 dBFS(A), ordinary kill at -23.76 dBFS(A), and catastrophic `TurretPop` at
-20.41 dBFS(A). The catastrophic event was 12.97 dB higher than the small impact in short-term
A-weighted level and 6.44 dB higher in the first 250 ms of broadband RMS. At 10 m the difference
was 10.54 dB(A) / 3.97 dB RMS, despite both touching the -1 dBFS limiter ceiling. These are signal
measurements, not a subjective listening verdict. Baseline log:
`artifacts/runtime/catastrophe_audio_baseline_20261001.log`.

Regression `reproduction::tests::catastrophe_retains_its_scale_through_propagation_and_master`
puts armor contact, ordinary kill and catastrophic pop-plus-rip into one eight-second mix with
the actual procedural blizzard wind voice. It covers 10, 22, 50 and 100 m exterior winter paths
and requires the catastrophe to exceed the small strike by 8 dB(A) and 3 dB in first-250-ms
broadband RMS after the shared master. It also requires catastrophe to exceed ordinary kill
and the summed output to remain finite and within the limiter ceiling.

## Actual first-person recordings

The same `blizzard-battle` command accepts `cupola` and `inside` after its duration:

```sh
KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=640x360 \
  target/release/panzerrechnersystem blizzard-battle captures/blizzard_cupola_20261001 14 cupola
KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=640x360 \
  target/release/panzerrechnersystem blizzard-battle captures/blizzard_inside_20261001 14 inside
```

Use a fresh prefix for every run. `preview` can follow the view name to record five video frames
per second while preserving 60 Hz simulation and 44.1 kHz audio.

The cupola run places the listener at the actual exposed commander post with the hatch open.
The inside run places the listener at the loader's canonical side of the basket facing the
breech, with the hatch closed. They use ordinary co-op station authority for the other automated
crew, including casualty gates; no listener-enclosure override or separate observer soundtrack
is used. Gun lay and initial placement remain disclosed review automation.

`Game::begin_audio_capture` records ordinary gameplay sinks. Each 1/60-second simulation step
consumes 735 stereo sample frames. The only output adjustment is removal of the common master's
70-frame lookahead latency. The selected station's headset, enclosure, propagation, reflex and
hearing damage remain active. Local listener death triggers the game's ordinary 1.5-second sound
fade, including when the crewman dies before the vehicle. This review holds the first-person
camera instead of entering the separate silent death replay. Runs use independent simulations;
crew occupancy and reload timing can change outcomes relative to the external solo-auto-crew
run. JSON receipts identify the viewpoint, listener KIA time, events, scope and live audio graph.

Heat-triggered detonation of the player's modeled ammunition reservoir now uses the same catastrophic blast/tearing voices as a direct rack hit. Ordinary individual wreck cook-offs keep their smaller secondary voice.

## Pressure body after the v6 crew review

The v6 crew recordings contain the own-gun report and all incoming hit cues. Their first ordinary
armor strike already has substantial 50–180 Hz energy. The fatal inside event is different:
94.7% of its first short onset window is below 50 Hz, and the whole run reaches 28.7 dB of master
limiting. These are measurements of exported audio, not a listening verdict or an attribution of
every overlapping voice. `captures/crew_v6_bass_diagnostic_20261001.json` preserves those windows.

An isolated healthy-ear render through the actual live graph confirmed a source problem in
addition to the internal-source routing problem. At 2 m with headset and without exterior hull
insertion, the old catastrophic source spent 79.1% of its first 250 ms energy below 50 Hz and
required 31.9 dB of master limiting. Its pressure waveform now retains one quarter of the
32–41 Hz displacement amplitude and adds a shorter 98–109 Hz compression/rebound at the same
time. This is one finite transient with audible body, not an added delayed boom or long oscillator.
The scales are authored timbre; they do not claim a solved physical pressure history.

With the same physical ear level, headset, reverb and common master, the 2 m catastrophic source's
50–180 Hz band rises from -16.12 to -11.56 dBFS (+4.55 dB), while total first-250-ms RMS falls
1.6 dB. Below-50-Hz share falls from 79.1% to 27.8%; master reduction falls from 31.9 to 27.1 dB.
The very close ammunition blast still limits heavily at its existing extreme source calibration.
No global gain, physical source levels, propagation table, headset, hearing model or master
parameters changed. The gun recording and ordinary plate impacts are unchanged.

`captures/destruction_pressure_before_after_final_20261001.json` records the controlled comparison,
input/output levels, band measurements and WAV hashes. The opt-in
`audio::calibrated::tests::destruction_pressure_path_review` exports the real-graph path matrix
when `KESSEL_DESTRUCTION_PATH_REVIEW_PREFIX` is set to a new capture prefix. Its healthy ears
separate source/transfer behavior from accumulated gameplay hearing damage. The same-compartment
22 m row is a transfer-shape diagnostic, not a literal interior distance. The body regression
checks the crew-headset path after calibration and the common master; the existing catastrophe
hierarchy regression covers the observer path. Actual final crew captures remain the acceptance
evidence for the combined routing and source changes.

Only the player's resolved rack detonation and heat-triggered ammunition detonation now mark
their TurretPop/TurretRip wavefronts as originating inside its fighting compartment. At arrival,
a Buttoned or HatchOpen listener in that compartment receives them without an exterior hull,
snow-ground or exterior-cover transfer. Distance, air absorption, headset, hearing damage/reflex,
travel time, steel interior reverb and the common master remain active. Enemy wrecks with the
same sound identity remain exterior. Head-out and on-foot paths keep the existing exterior
approximation; directional escape through the actual hatch aperture is still unmodeled.
`game::listener_hearing::tests::internal_ammunition_source_keeps_headset_without_crossing_its_own_hull`
guards provenance, enclosure selection and headset retention.
