# Steel impact ring: DSP revision, 2026-09-05

The requested sound direction is a massive struck steel enclosure with a sustained, inharmonic
ring. The prior armor source explicitly prevented sustained modes. This revision restores them;
it is authored procedural sound, not a calibrated Tiger acoustic or structural simulation.

## What the old capture actually used

`src/astra_capture.rs::audio` calls `acoustics::synth_damage_impact_body`, adds selected physical
fragment contacts, a quiet filtered ambience bed and separate hearing-fatigue audio. At this audit
it mixed the main at 0.94, contacts at 0.18 and fatigue at 0.10, then applied another tanh. The main
source already applied several nonlinear soft-limit stages. The root capture work is responsible
for replacing that last tanh with fixed gain and an attenuation-only overload guard.

The old `synth_impact` used only finite Ricker wavelets and suppressed useful structural energy
beyond about 0.2 seconds. Its 0.3–0.5 second buffer length was largely silence. The damage rack mixed
pressure at 0.88 and each casualty fall at 0.85. There was no sustained stochastic excitation in
the armor voice: the perceived firecracker was consistent with a short pressure/contact stack
without the requested steel decay, rather than evidence that it was literally white noise.

Preserved authentic mixed baseline:
`captures/astra/interiors_20260905_clean_probe_02_witness_cupola_open_85mm_left_soundtrack.wav`.
It is six seconds, stereo, 44.1 kHz, signed 16-bit. Its peak is 0.3790; thus it is not digitally
clipped at the WAV boundary. RMS for the first 0.2 s after the hit is 0.11485; at 2.7–3.2 s after
the hit it is 0.000543, approximately the quiet ambience/fatigue floor. These waveform checks do
not establish subjective quality or playback loudness.

Baseline WAV SHA256: `a64334437b38b91a2a2f6289471dec54a670be3068bfcbc43a72f0e8175bf974`.
Pre-edit source was preserved at `/tmp/panzer_acoustics_before_modal_20260905.rs`, SHA256
`23592b0ce7bca273b9aebd1eb54f5fbcb176ed7a6674c420a61b1881f2cad5b4`.

## New source behavior

- Sixteen unevenly spaced structural partials carry the event. Each mode has independent damping;
  upper partials decay faster. Oscillator recurrence avoids per-sample trigonometric/exponential
  calls. The buffer lasts 4.45 s for a held hit and 3.45 s for a penetration, with a quiet final fade.
- A short, subdued contact excites the ring. It contains no stochastic excitation and no tanh.
  Shell velocity controls the contact envelope/high-mode excitation; caliber shifts spectral
  weight, and energy, incidence, thickness and residual effect change the response.
- Panel class supplies the available structure/material/location proxy. The current API has no
  local strike coordinates, mesh eigenmodes or armor-alloy constants; deterministic detuning is
  variation, not a computed impact-position mode shape.
- Bearing still controls constant-power panning, interaural delay and far-ear filtering. Listener
  distance still changes level and now also darkens the plate response slightly.
- Penetration increases damping without silencing the surrounding structure. The existing damage
  profile still distinguishes a dry breach, filler burst and catastrophic event. Its pressure is
  mixed at 0.22 times the available distance attenuation; casualty falls at 0.22; optional authored
  fragments at 0.18. Actual physics-driven fragment contacts remain separate.
- The final damage main uses fixed gain 0.82. `attenuate_over_peak` is transparent below the ceiling,
  never boosts a quiet event, and scales overloads linearly instead of adding distortion harmonics.
  It is digital headroom protection, not an assurance about headphone sound pressure.

The witness metaphors retained in `docs/research/inside-the-steel-box-report-source.md`, especially
its Sound section and named Degen/Aria accounts, informed this direction. They do not provide
measured modal frequencies, decay constants, physical pressure or a universal Tiger sound. No new
historical quotation or claim of measured validation is introduced here. Hatch transfer remains
unmodeled in this synthesis.

## Verification and reproduction boundary

The actual `src/acoustics.rs` and `src/ballistics.rs` were included by path in an isolated Rust test
harness and linked to the already-built glam library. All 42 tests passed on one CPU thread in
0.75 s. No Cargo invocation, GPU render or game build was used for these checks. Tests cover
resolved late spectral peaks rather than diffuse noise, sustained then dissipating modal energy,
finite deterministic stereo, ear delay beyond a constant channel gain, caliber/energy/angle/
thickness/distance response, penetration damping, distinct consequence profiles and transparent
headroom. The old coarse spectral sampling was refined to resolve narrow modes; its old demand
for silence immediately after the impact was replaced by the requested decay behavior.

The old v1/clean-probe manifests record velocity, residual velocity, seed, bearing and distance,
but omit full ImpactParams and DamageSoundContext (including module hits and catastrophic state).
They therefore cannot reproduce an exact same-event DSP comparison without assumptions. Preserve
the old WAV as authentic evidence; serialize both full structs in new captures. The next user
review must be the fresh synchronized witness video using its actual recorded event. Audio-only
comparisons are not the requested deliverable, and this CPU verification does not establish that
the resulting sound has met the user's taste.
