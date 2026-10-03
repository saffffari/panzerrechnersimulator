# Reference audio first implementation slice — 2026-10-02

Only the isolated `reference-pass-20261002` source tree changed. The running MAIN
game and its audio device were not touched. This is a source checkpoint awaiting
the coordinated CPU tests, integration build and first-person audition.

## Implemented source changes

- Local per-round MG receiver/mount response retains the existing estimated
  MG34 crew-reference level, actual shot cadence and headset transfer. Its
  78 ms mechanical response gains short lower structural modes. The former
  Schroeder room pass added 29–44 ms comb returns and a 0.38 s tail to every
  round; removing that pass from this local structural voice addresses a
  concrete source of ringing. It is a code-supported cause hypothesis for the
  reported hollow sound, not an audition result. The independently propagated
  exterior muzzle report still crosses its actual path and environment.
- MG receiver preparation now caches twelve source variants with the exact nine
  transfer-band bit patterns, at most 32 entries per game thread. Repeated rounds
  reuse PCM and measured level instead of synthesizing, fitting spectral filters
  and measuring again. Capture metadata reports cache hits/misses and maximum
  preparation time, explicitly **not** an ALSA underrun count. Rodio's default
  device configuration and the host buffer policy remain unchanged. Runtime
  underrun clearance has not been demonstrated.
- P08 direct gameplay reports now use an independent 105 ms source bank. Only
  the early 45 ms attack from the verified owned generic pistol source may enter
  it; authored pressure/modal response provides the remainder. The original
  source was not identified as a real P08 recording. No user reference audio was
  copied into runtime assets. Existing pressure propagation and source level
  remain authoritative. Rifle/MG reports receive a short pressure shoulder
  without a sustained noise bed or cannon sub-bass.
- Distinct loaded Tiger report variants now reach playback instead of all being
  replaced by the same generic explosion recording. Authored cannon pressure
  voices gain modest charge/modal variation. No source-level boost was added.
- Empty cases and live rounds use different contact modes and decay lengths.
  Contact level follows the actual supplied kinetic-energy terms around a named
  authored reference. One call produces one contact; subsequent real rebounds
  must call again. Hatch-stop and pistol-magazine samples no longer represent
  these ammunition contacts. Extraction/ram friction is a continuous voice driven
  by signed actual movement; end cues are short release/seating contacts.
- Procedural Maybach cadence now maps authoritative 800–3000 RPM to the source
  clock. Load changes chuff/gear/lash character and its calibrated whole-source
  level, without adding a second artificial pitch droop. Player track contact
  modes and intermittent friction follow actual belt motion and turn demand.

Root owns the real loader/case-contact calls described in
`REFERENCE_AUDIO_HOOKS_20261002.md`. The new enemy running-gear pool is available
but not integrated in this urgent slice; the old enemy track layer was retained
to prevent a regression to silent tracks. Removing its fixed cadence remains a
follow-up requiring actual vehicle-motion input.

## Preserved systems and remaining work

Physical wavefront timing, source-level calibration, hull/headset filtering,
Mach-cone cracks, projectile terminal timing, common limiter, outdoor environment
tails and live audio capture are preserved. Source observations support the MG
comb-tail diagnosis but do not prove subjective realism or explain every ALSA
underrun. The supplied videos/WAV are reference material, not absolute SPL
calibration or redistributable runtime samples.

The full AUD-01–12 checklist remains active. Material/energy-driven distant
terminal reports, broader environment reflection modeling, moving enemy track
integration, finite-fuel fire variation and the remaining sound design pass are
not claimed complete by this checkpoint. P08, MG and case timbres need live
first-person listening after tests; authored modal estimates are not measurements
of the particular historical mechanism.

## CPU regression filters

Run from the coordinated current test executable (root owns Cargo):

```text
audio::small_arms::tests
audio::ammunition::tests
audio::calibrated::tests::local_receiver_cache_preserves_exact_transfer_and_has_no_room_tail
audio::engine::tests
audio::running_gear::tests
ammunition_contacts_use_their_own_modal_voice_in_the_actual_mixer
tiger_reports_keep_distinct_loaded_variants_without_generic_explosion_replacement
audio::metadata::tests
```

The helper-source `rustfmt --check` parsed without syntax errors; formatting
differences were not applied. This document does not claim any tests passed.
