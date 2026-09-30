# Structural decay of the armour impact voice — 29 September 2026

The impact synthesizer previously assigned its lowest partial about 5.8 seconds to decay by
60 dB, with higher modes decaying only as the mode ratio raised to 0.43. The result retained
many upper partials long after the initial strike. This is a timbre approximation, not measured
Tiger I vibration or pressure history.

The replacement uses the structural loss-factor relation

`amplitude(t) = amplitude(0) exp(-pi f eta t)`

and therefore `T60 = ln(1000) / (pi f eta)`, approximately `2.2 / (f eta)`.
This is structural vibration decay. It is separate from the airborne room reverberation and
the existing early-reflection taps. The 0.1–0.3 s entry in the older sound-level table is an
estimate; it does not justify forcing every hull mode to the same decay time.

Evidence and limits:

- Sarlin et al., *Vibration damping properties of steel/rubber/composite hybrid structures*,
  report steel-sheet loss factors around 0.005–0.01 and discuss contributions from the test
  mounting. This is an experimental reference for steel sheets, not an assembled Tiger hull.
  [Author repository manuscript](https://researchportal.tuni.fi/files/5867723/Sarlin_2012_CS.pdf).
- The structural decay relation is also used in experimental vibroacoustic characterization,
  Eq. 15 of *Vibro-Acoustic Characterization of a Composite Structure Featuring an Innovative
  Phenolic Foam Core*. The formula is applicable here; that paper's specimen properties are
  not transferred to armour.
  [Primary paper](https://www.mdpi.com/2076-3417/9/7/1276).
- Hryciów et al.'s measured/APC hull study demonstrates that connected plates, supports and
  the turret bearing affect structural modes. It supplies no Tiger I modal damping value.
  [Primary paper](https://pmc.ncbi.nlm.nih.gov/articles/PMC8618401/).

The implementation's effective loss factor `eta = 0.01` is explicitly **an inferred timbre
parameter**. The existing relative support/thickness/breach factors remain. Without those
factors, the resulting T60 is 2.20 s at 100 Hz and 0.22 s at 1 kHz. Lower structural modes still
carry weight; higher partials no longer share a long bell-like tail. This does not solve modal
frequencies from the Tiger mesh, its welds, load paths, material hardness or boundary conditions.

Unchanged: hit energy and penetration state, initial modal amplitudes/frequencies, simultaneous
contact/body/pressure layers, source sound-pressure law, reproduction LAF calibration, hearing,
the owner's selected gun recordings, and fragment trajectory/contact sound scheduling. The
waveform's measured LAF may differ; the existing calibration still places it at the same requested
ear level. Tests compare the first 8 ms and the 0.3–1.2 s upper-mode tail with the prior law,
and retain the directional, headroom and concurrent-layer checks. In-game sound judgment remains
pending a synced player-view encounter.

## Confirmed internal explosions

The owner's subsequent review identified the catastrophic-penetration bell as the wrong event
identity. The runtime now selects an internal explosion only from a penetration whose resolved
`DamageSoundContext.internal_detonation` is true. Ammunition catastrophe additionally requires
`catastrophic`; a severe solid-shot injury, casualty count, shell class or destroyed module alone
cannot select it. The game's flag uses the same modeled non-HEAT filler energy as its anatomical
damage pass, or the live-ammunition contact outcome. The waveform and ear-pressure selection use
that flag consistently.

A confirmed filler/ammunition burst receives a finite broadband pressure/rupture response and
hot-gas release, with lower modal weighting (0.28 for filler, 0.10 for ammunition catastrophe).
Contact and plate body remain immediate; the explosive air component follows the existing
pressure-front delay. The entire group is calibrated once through the unchanged source dB law,
LAF reproduction transform, ear model and master limiter. It adds no second independently
calibrated explosion, no pre-hit ducking and no duplicate synthetic fragment shower. The actual
fragment contacts remain their own event voices.

The burst envelopes, bandwidths and relative layer weights are authored timbre estimates, not
measured Tiger pressure histories, detonation chemistry or a deformation solver. A short hot-gas
tail does not assert a sustained fuel fire. `impact-mix-review` adds an explicitly staged
ammunition-outcome comparison at the existing 122 mm filler's ear level; that test isolates
timbre and does not claim to resolve or calibrate a full ammunition-rack detonation. Tests guard
event identity, broadband response, finite decay, delay, deterministic live/offline parity and
the existing output ceiling.
