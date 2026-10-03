# Owned audio library catalog — 2026-10-02

The user explicitly authorizes these owned Splice samples for use in the game. This records
that declaration; it is not an independent review of license documents or source identity.
Originals remain unchanged under `/home/alex/panzer/panzer_sounds`.

The latest stable full-source inventory contains 163 PCM WAV files, 1,017,526,458 bytes, reduced by exact file
SHA-256 to 155 distinct assets and eight duplicate aliases. This is byte identity, not a claim
that differently packaged recordings are perceptually distinct. All files decoded successfully.
No sample has been represented as listened to or subjectively approved by this catalog pass.

Runtime catalog: `assets/audio/library/manifest.json`. Stable IDs use a 96-bit prefix of the
complete source SHA-256, with a collision check and full hash retained. Canonical clean names
are preferred over `(1)` copies; explicit night-folder membership survives cross-folder aliases.
All filename-derived roles, BPM and keys are marked as unverified inference. No tempo or pitch
analysis is claimed. Missing values remain null.

## Active night pool

The explicitly selected `night_ambiance` folder is enabled by default. Its seven files are
five distinct recordings; duplicate names do not receive extra selection weight.

| Recording | Duration | Source RMS | Sample peak | Suggested runtime gain |
| --- | ---: | ---: | ---: | ---: |
| D_-_Calm_Dusklife_1 | 34.1337 s | -27.173 dBFS | -14.173 dBFS | -2.827 dB |
| D_-_Corrupted_Dusklife | 42.3257 s | -26.829 dBFS | -11.525 dBFS | -3.171 dB |
| D_-_Timid_Dusklife_1 | 42.3257 s | -26.610 dBFS | -12.173 dBFS | -3.390 dB |
| D_-_Timid_Dusklife_2 | 42.3257 s | -26.269 dBFS | -12.069 dBFS | -3.731 dB |
| Nighttime_Wanderers | 54.6133 s | -25.661 dBFS | -10.851 dBFS | -4.339 dB |

These are stereo 48 kHz, 24-bit source files. The five playback derivatives are stereo 44.1 kHz
PCM16 WAV, with triangular quantization dither and no baked attenuation or trimming. Their
separate hashes, formats, analysis and exact FFmpeg recipe are retained. Runtime gains only
attenuate toward -30 dBFS combined-channel RMS and below a -12 dBFS sample peak; they are a
starting reproduction mix choice, not physical sound pressure, LUFS normalization or final
listening acceptance. Runtime bus gain and combat ducking remain separate policies.

The initial four Dusklife sources have 0.68–0.80 seconds of exact digital silence at the tail. Zero/tiny end-to-start
sample jumps therefore do not demonstrate a seamless bed: a hard wrap can have a level dip.
The suggested full-file loop overlaps the last and first two seconds with equal-power weights
(the measured edge correlations are small). This preserves the original program and can bridge
the silent tail, but requires audible validation for pumping, fade gaps and repeated landmarks.
The manifest never labels these loops perceptually accepted. Loop frame coordinates refer to
the derived playback sample rate once imported, not to the 48 kHz original frame count.

Selection policy is one content-unique bed per night, varied with the run seed and avoiding an
immediate repeat where possible. Catalog data does not itself implement or validate the runtime
selection, transitions, weather response or mixing engine.

## Additional scenes and score material

The user subsequently organized additional folders with explicit runtime roles:

| Folder | Runtime role | Selection scope |
| --- | --- | --- |
| `calm_ambiance` | `explore` | Exploration score pool |
| `combat` | `combat` | Combat score pool |
| `suspense` | `threat` | Threat score pool |
| `interior_tank_ambiance` | `cabin` | Independent inside-tank layer across other modes |
| `music` | `music` | Melodic program; per-cue loop/one-shot decisions |
| `game_start_atmosphere` | `intro`, one-shot | `run_start` event registration |
| `death_sounds` | `stinger`, one-shot | `death` event registration; dedicated death presentation is integrated separately |
| `effects` | `effect`, one-shot | Registry only, with no invented gameplay trigger |
| `winter_atmosphere` | `day`, context `winter` | Winter environmental pool |
| `summer_atmosphere` | `day`, context `spring` | User-approved alias until an actual summer mode exists |
| `sunrise` / `sunset` | `stinger`, finite | Named solar-horizon events; pending when no source exists |
| `heavy_blizzard` | `blizzard`, loop | Separate physical near-whiteout weather layer |
| `scene` / `scenes` | `scene`, disabled | Requires an explicit location binding |

These assigned folders are enabled and imported alongside the five night beds. Score pools
use attenuation-only -26 dBFS RMS/-12 dBFS sample-peak alignment; cabin beds use the -30 dBFS bed
target. BPM-labelled cues preserve their full source period without a two-second loop overlap.
The filename BPM remains an unverified metadata claim, not a measured tempo. Runtime selection
and harmonic/transition decisions still need listening acceptance.

This final snapshot imports69 distinct cues. Current runtime policies assign explore8, combat9,
cabin11, night5, threat10, music2, intro8, stinger4, effect6, and seasonal day6. The PCM16
derivatives total426,391,384 bytes; decoded stereo float32 would occupy852,776,696 bytes if all
were resident simultaneously.
The runtime must decode lazily or select a bounded review fixture; the catalog does not require
loading the entire library. Six scene recordings remain disabled. No originals were moved or
changed. The stable scan reported no failures or moving-source warnings.

`Cavern_Wraiths_4.wav` occurs identically in intro and suspense folders. The catalog retains one
content identity and both `folder_assignments`; the canonical role is intro. The supplemental
threat binding is explicit in the cue-policy proposal so this authored intent is not lost or
given accidental duplicate selection weight within one pool.
`role_aliases` includes every assigned role. `role_contexts` maps each role to alternative
lists of required contexts: inner lists are AND, alternatives are OR. This preserves authored
membership without incorrectly requiring both winter and spring for a shared recording.

## Provisional cue policies

`catalog/soundscape_cue_policy_20261002.json` contains the numerical basis and quarter-second
envelopes for all69 enabled cues. `catalog/soundscape_runtime_bindings_20261002.json` is a small
merge proposal, not the live `assets/audio/library/soundscape.json`. Neither file claims listening
acceptance or installs arbitrary event triggers. Regenerate both with
`python tools/suggest_soundscape_cue_policy.py` after catalog/import.
The frozen proposal has now been merged into that runtime configuration with no invented scene
zones; subsequent regeneration still requires explicit integration rather than overwriting it.

The proposal has49 loops and20 one-shots. All five night recordings and all cabin beds loop;
their measured fades are not treated as evidence that a persistent cabin layer must disappear.
Ancient_Instrument uses a50ms onset and1s exit fade as a one-shot; its last quarter-second falls
to roughly-96dBFS. The unlabelled wind-chime music recording provisionally plays once. The user
explicitly defines a standalone `_loop` filename token (suffix or between underscores, including
source aliases) as a repeat instruction. The VHS recording therefore repeats its complete file,
although its50.136-second source contains133.70 nominal beats at the claimed160BPM and a long
fade ending near-86dBFS. Beat alignment/end trimming is flagged for authoring; no shortened
musical loop region has been invented and no overlap changes its complete-file period. In
contrast, the8-second fxlp120 sources contain exactly16 nominal beats and keep their complete
period without overlap. BPM is filename metadata, not measured tempo.

`_loop` overrides default one-shot recommendations for reusable effect or stinger cues too;
the terminal Death presentation still plays finitely by its separate runtime rule. A loop marker
does not enable an otherwise unassigned source. Unlabelled atmospheres can retain their normal
bed-loop policy. This update uses the frozen manifest and aliases, without rescanning folders
while the user continues organizing them.

## Final bounded refresh and ending tails

One final catalog invocation accepted a stable snapshot after one moving-source retry; no watcher
or repeated rescan remains active. The original53 enabled cues survived by content identity,
and16 newly assigned cues were added. Authored runtime fields are preserved by a three-way merge
against the previous proposal; a regression checks custom gain, rack, event, and location fields.
Current source paths determine usage. Historical paths remain provenance, with `_loop` the
explicit repeat exception retained across alias renames. See
`catalog/soundscape_filename_usage_20261002.json` for exact label/rename evidence.

`combat_start` is a finite stinger registered under the same named event, outside the continuous
combat score pool. It is not bound to factual combat-mode entry or an enemy detector; the runtime
may choose it once per actual incoming-contact episode. `surprise_contact` and `riser_transition`
are imported effect entries with no trigger binding and no continuous score-pool membership.
All eight opening atmospheres preserve their complete authored duration and play once in the
intro role. Actual incoming combat may interrupt with the authored fade; metadata imposes no
arbitrary maximum opening duration.

Eight finite phrases receive provisional ending-only racks: Ancient_Instrument, combat_start,
surprise_contact, and five intro textures (Ascending_the_Pillars, Entering_the_Cove,
Heart_of_the_Valley, Cavern_Wraiths_4, Temple_Halls). The instrument uses wet0.12/decay1.2s;
the others use wet0.08/decay1.1s, with14ms predelay and4500Hz damping. `tail_only_s` is3s for
the long phrases and1.2973s/0.96s for the short combat accents. Their terminal windows either
drop52–66dB abruptly while the last2s remain energetic, or reach EOF at approximately-24/-12dBFS.
These measurements identify candidates, not proof that the recording lacks existing reverb.
The restrained rack applies only at the ending; full-cue dry output is preserved before it.

Looping beds, drums, and already-decaying death/synth recordings stay dry. The death chords have
roughly9–11s existing decays and end below-64dBFS in their last2s; adding a second room was not
justified numerically. Runtime support for `tail_only_s` and preserving its finite post-source
tail is integrated separately; the catalog does not claim audible acceptance. The final receipt
`artifacts/runtime/audio_library_final_tail_validation_20261002.json` verifies69 hashes/formats
and16 metadata tests. No Cargo or GPU job was run for this import.

## Targeted solar and blizzard additions

Two subsequent targeted category imports leave the other source directories unscanned. Runtime
now has71 cues. `sunrise/sunrise_sound_start.wav` reuses existing content ID
`audio_d4629c7792488f4b25cca7b4` from the formerly disabled PHOTAY texture:40.379478s, finite,
150ms onset/1s end envelope, dry rack. Its measured last2s are-78.47dBFS, so no extra reverb
was justified. `events.sunrise` contains that cue; `events.sunset` is empty because the targeted
scan found no sunset source. The solar-crossing trigger is a separate runtime integration.

`heavy_blizzard/Storm_Nexus.wav` is36.194444s and uses the separate physical `blizzard` role,
looping with3s onset/4s exit fades and no extra reverb. It has no impossible `heavy_blizzard`
context-tag requirement and cannot enter day/night/calm/music pools. Runtime gates this layer
using actual near-whiteout weather. Original69 cue dictionaries and their eight ending-only
racks remain identical. Current derived totals and hashes are in
`artifacts/runtime/audio_library_solar_blizzard_validation_20261002.json`;19 metadata tests pass.

For a later explicitly requested targeted update:
`python tools/import_soundscape_categories.py sunrise sunset` analyzes only those top-level
directories and reuses source content identities. It does not run a watcher or replace runtime
configuration; policy regeneration and the three-way merge remain separate steps.

`gain_db` in the proposal is an additional runtime offset **after** the manifest's suggested
attenuation, with total attenuation recorded separately. Existing within-file dynamic range
is preserved: there is no compression, loudness riding, baked normalization, or positive gain
boost. One dense7.1dB-crest intro receives a provisional extra-3dB offset. End-fade suggestions
are not permission to truncate original samples. Whole-program mixing and all seams need
audition before subjective acceptance.

Other current cues remain disabled unless explicitly requested for import. The catalog
recognizes provisional scene beds, environmental beds, score rhythm/percussion/tonal/accent
material, world-effect candidates and unreviewed textures. Cave, ship, fantastical texture,
bird and summer names do not authorize automatic use in the winter battlefield.

Location tags support future composed scenes such as coffee shops, playgrounds, airports,
farmhouses, crowds and conversations. A filename is not proof that a file contains people or
any particular location. New recordings require listening/context review and an explicit scene
binding. A scene can layer a bed, room tone, spot effects, dialogue and score independently;
binding/spatialization belongs to the runtime rather than being inferred from a filename.

Category subfolders are retained as `folder_contexts` and normalized to lowercase underscore
`contexts`: `combat/winter/night/file.wav` carries `["winter", "night"]`; files directly under
the category carry an empty list. `scenes/airport/...` has role `scene`, tag `airport`, and remains
disabled until bound. This metadata supports the runtime's most-specific-context selection
with general-pool fallback; the catalog does not invent new conditions for unknown subfolders.

## Repeatable import

Run from this checkout:

```sh
python tools/catalog_panzer_sounds.py \
  --source /home/alex/panzer/panzer_sounds \
  --output assets/audio/library/manifest.json --import-enabled
```

Import a specifically selected existing source without enabling its neighbors:

```sh
python tools/catalog_panzer_sounds.py \
  --source /home/alex/panzer/panzer_sounds \
  --output assets/audio/library/manifest.json \
  --enable-source 'oe_fxlp120_playgroundambience2.wav' --import-enabled
```

`--enable-source` is repeatable and relative to `--source`; an unknown/outside path is rejected.
It sets `default_enabled` for that content without silently replacing its inferred role; runtime
cue bindings may assign the intended role explicitly. CLI import choices outside assigned folders must be supplied
on subsequent regeneration if they should stay default-enabled. Previously imported, registered
derivatives remain available even when later disabled. Disabled, never-imported assets have no
`playback_path`, so a catalog entry cannot imply an existing playable file.

An alternate source folder and output manifest can catalog a future scene collection separately.
All imports stream decoding/resampling. Inventory analysis reads 65,536-frame PCM blocks and
bounded two-second head/tail buffers, not the entire ~1 GB collection into memory. Original PCM
metadata and hashes are preserved; only enabled cues are converted. Source recipes and output
hashes allow an unchanged import to reuse existing playback files. No source file is deleted.
If the user reorganizes folders during analysis, the tool retries bounded fresh snapshots by
content identity. If movement continues, missing sources are skipped with explicit warnings;
rerun after organization finishes for a complete stable inventory.

Numerical validation: `python tools/test_catalog_panzer_sounds.py` and
`python tools/test_soundscape_cue_policy.py`. These tests cover 24-bit sign
extension/channel order, streamed level/silence measurements, content aliasing and explicit
import scope. They do not substitute for listening to the runtime blend.
# Targeted damaged-engine hiss import (2026-10-02)

One effects-only inventory found five WAVs. Only the explicitly named
`effects/destroyed_engine_hiss.wav` was imported in this pass; the four generic
effects received no new triggers. It is the unchanged content previously catalogued
as disabled `Ship_Cooldown.wav`, retaining id `audio_098c00adbf41258d06e7133c`.
Its full 27.306667-second source is stereo 48 kHz/24-bit; the playback derivative is
stereo 44.1 kHz/16-bit, with no baked attenuation or trim. Catalog attenuation is
-4.831382 dB and the additional runtime offset is zero.

The new `engine_hiss` role is physical and excluded from every music/general-effect
pool. The current filename has no `_loop` marker, so its default is finite, with
80 ms onset and 400 ms interruption/end fades, dry processing, and no arbitrary
retrigger. The runtime owns the actual damage/repair trigger, source position, and
distance attenuation. Explicitly loop-labelled future hiss cues can sustain; their
complete period is preserved. This file has energy throughout most of its duration
(last two seconds -24.94 dBFS versus peak quarter-second -22.01 dBFS), then 136 ms
of digital silence. Those measurements do not prove a seamless sustain loop or
identify the recorded machinery; no listening claim is made.

The library now has 72 runtime cues. All previous 71 per-cue dictionaries, eight
tail racks, events, and other runtime settings are unchanged. Twenty-one Python
metadata tests pass; the Rust `engine_hiss_is_physical_and_repeat_is_explicit`
regression is saved for the coordinated build. Evidence is
`artifacts/runtime/audio_library_engine_hiss_validation_20261002.json` and the
corresponding import/test files. Originals were only read; no Cargo or GPU work ran.

## Separate dusk event (2026-10-02)

A single targeted `dusk/` scan imported `dusk/sundown.wav`, retaining content id
`audio_c738a81f08c3b22ff78aa296` from the previously disabled PHOTAY arpeggio source.
The 29.024921-second source is stereo 44.1 kHz/24-bit; playback is stereo
44.1 kHz/16-bit, with the complete duration preserved. It is a finite `stinger`
bound only to named event `dusk`, separate from `sunset`. Runtime owns the actual
descending -6-degree solar crossing; this metadata change does not implement that
trigger. Onset/end fades are 0.15/1 seconds. The measured final two seconds are
-59.98 dBFS versus peak quarter-second -14.76 dBFS, so no second reverb is added.

There are now 73 runtime cues. All previous 72 per-cue settings, eight tail racks,
and earlier event lists are unchanged; `sunset` remains empty/pending its own source.
Twenty-two Python metadata tests pass. Original hashes and the new playback hash
were verified, without a listening claim, broad rescan, Cargo, or GPU work.
Receipt: `artifacts/runtime/audio_library_dusk_validation_20261002.json`.
