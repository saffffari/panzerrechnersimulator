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
