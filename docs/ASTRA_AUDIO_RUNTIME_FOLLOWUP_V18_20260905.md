# Remaining audio follow-up — v18 source, 5 September 2026

**Source prepared; compiled tests and video/listening gates pending.** No Cargo, GPU rendering,
new sound export or audio-only comparison was run by this lane. Existing media and all eight
recording recipes are preserved.

## Confirmed live fixes

`Game::enemy_fire` scheduled the caliber-generated cannon report and a second generic `EnemyGun`
for the same muzzle discharge below 120 m. The latter is removed from the live function only.
The caliber report, source position, seed, distance attenuation and distance/343 arrival remain
unchanged. The `cfg(any())` migration reference still contains its old code and never ships.
The new `live_enemy_discharge_has_one_report_without_a_close_range_duplicate` gate isolates the
active function and asserts exactly one report/queue entry plus the retained propagation inputs.
It does not alter armor-impact, projectile-contact, tank-MG or auxiliary HE firing routes.

`Audio::set_master_volume` updated transient tails, while hatch-motion, starter and jam sinks were
outside that collection. Those three mechanism types retained their creation gain through a
subsequent concussion volume change. They now update their existing sink gains in place, retaining
phase, waveform and the established unity gains (0.75 hatch/starter, 0.60 jam). The new
`active_mechanisms_follow_listener_gain_without_restarting_their_motion` gate exercises actual
capture sinks, checks gain changes and unchanged creation count, and confirms ordinary stop calls
still release the sinks. Approved engine, ambience, tank MG and impact voices are untouched.

## Assets reread, not blindly substituted

[Fresh source-byte/recipe receipt](../artifacts/astra-audit-20260905/audio-source-followup-v18.json)
rechecks all eight runtime OGGs against both their exact extracted originals and catalog fingerprints.
All match. The game-private original root is
`source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted`.
The selection stays in `catalog/audio_feedback_20260905.json`:

- Hatch movement: Echoes v2.0.1, `OGG/SFX/Doors & Drawers/heavy_door_open-close_1.ogg`,
  SHA-256 `3f9b3966d8afd0abe3927019496918d53a950ae59a2713f12bce521eb8d402bd`.
- Hatch endpoint: Rust & Blood v1.1d, `OGG/Sound Effects/Impact & Break/Metal/impact_metal_ufx_1.ogg`,
  SHA-256 `f4bb037fc1bde8d5a5f6657fa40e81295fd9e9eee39412e06fbcebf4335d3099`.
- Trigger: Rust & Blood, `Firearms/Misc/firearm_universal_safety_switch_ufx_1.ogg`,
  SHA-256 `daf7e38fd682b515f368633d5971207d784bc5aa5958a30be9423302b183223f`.
- Interior cannon: Rust & Blood `Explosions/explosion_close_short_ufx_1.ogg`,
  SHA-256 `2be44528721ca5e3076e97498a881a8d953029dcae7fd14a6be1ff668973ff38`.
- Exterior cannon: Rust & Blood `Explosions/explosion_close_short_ufx_2.ogg`,
  SHA-256 `baad4f0fdff4425e0da2c422a1bb14e4a34ae3cedd92e94b51cf64bbc79bd0d4`.

Approximate source-recipe analysis uses an independent FFmpeg float decoder and the same documented
pitch, two-pole rolloff, bass shelf and envelope. It is not byte-identical rodio mixer validation or
an audition. None of the eight processed sources exceeds unity in this analysis. Interior cannon
peak is 0.727, with 93.8% of energy below 180 Hz; exterior peak is 0.846, with 88.8% below 180 Hz.
Energy above 4.5 kHz is negligible for both. Their generated recoil companion is a separate cue and
must be assessed in the combined first-person video. No further bass boost, normalization or sample
substitution is justified by this source-only evidence.

The scoped extracted OGG inventory has no starter/crank/ignition/engine-named recording. Generic
machine loops are available but do not establish a matching starter motor. The current authored
1.8 s geared/compression approximation stays in place. `cue_engine_death` already cancels the crank
when the engine is destroyed; an explicit second ignition-press cancellation action is still absent.
The existing engine-loop transition supplies the catch; no separate historical starter/catch sample
is claimed.

Two `rifle_universal_empty_barrel` recordings and a second safety-switch recording are retained
unselected candidates with exact hashes in the receipt. A filename alone does not make them a more
accurate Tiger trigger. The present cue already fires once per eligible deliberate pull, including
an actual non-igniting cartridge primer and a blocked gun. The primer lifecycle and dedicated
15-second replacement-cartridge recipe were added by the separate equipment lane and are preserved.
Hatch movement already follows measured phase, stops when motion stops, and adds its metallic
endpoint only at arrival; a reversal is not an arrival.

## Required first-person verification

Use the existing nineteen-spec audio/equipment queue plus the primer-fault recipe. Preserve approved
45 mm and 85 mm impacts, KV-2 MG, ambience and driving as comparisons. Verify a live enemy shot below
120 m emits one audible report at its muzzle-arrival time; verify a farther shot retains its former
voice. The impact sound occurs later at actual contact and is not the removed duplicate.

Record hatch opening, closing and reversal with endpoint timing; crank into running and cancel via
an actual engine failure; deliberate disabled/misfiring trigger pulls followed by release; and a
normal cannon shot with its recoil. Listen to the actual video and review source/mixer metadata.
Numerical energy, sink-control tests and matching file hashes do not constitute sound approval.
