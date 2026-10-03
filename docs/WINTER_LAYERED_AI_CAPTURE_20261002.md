# Fresh winter AI and layered-audio captures

The final pre-launch routing on 2026-10-03 has 137 enabled cues, following finite-score
integration and explicit deferral of six unassigned Foley/error sources. Captures described
below as using 143 cues retain that earlier source/configuration identity; they do not
retroactively demonstrate the final 137-cue routes. See the final section at the top of
`docs/SORTED_AUDIO_LIBRARY_20261002.md` and its separate promotion receipt.

The first three recordings are complete; their individual receipts include two retained
failed diagnostics. They are not blanket visual/listening acceptance. Run further captures
from `/home/alex/panzer/adaptive-audio-20261002` only after the new release is built.
The built-in recorder runs real Game updates, AI, ballistics, damage and ordinary
Audio sinks. Controls and opening setup are disclosed. No sound overlay, hit,
kill, invulnerability or AI awareness is injected. Wall time is not a frame-rate benchmark.

```sh
export KESSEL_ASSET_ROOT=/home/alex/panzer/adaptive-audio-20261002
export KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_GRAPHICS_PRESET=high
export KESSEL_FIXED_PLAYTEST_GRAPHICS=1 KESSEL_INTERNAL_RES=800x450
export KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4
export KESSEL_SOURCE_VISIBILITY=maps KESSEL_TRANSPORT_QUALITY=full
export KESSEL_ENABLE_PIZZA_AUDIO=1 KESSEL_AUDIO_RANGE=reference
unset KESSEL_COMBAT_STILL_ONLY

target/release/panzerrechnersystem combat-scene-suite captures/layered_winter_night_v1_20261002 winter_canyon_night_drive 20
target/release/panzerrechnersystem combat-scene-suite captures/layered_winter_contact_v1_20261002 winter_canyon_infantry 30
target/release/panzerrechnersystem combat-scene-suite captures/layered_winter_blizzard_v1_20261002 winter_canyon_blizzard_close 30
```

Run sequentially; retain failed receipts and videos. Do not choose seeds from combat outcomes.
Use a fresh prefix if an output already exists. Do not pass `preview` (that mode is5fps).
All three encode native800×450 at30fps, with60Hz simulation and44.1kHz stereo audio.

- Night: full live seed29 Canyon course at unmoved authored spawn, four occupied
  mortal co-op roles, Commander head-out camera; Driver0.30 throttle after0.5s,
  no own gun/aim commands. Weather0.10, astronomical Night. Existing terrain and
  obstacle constraints can stop it. Night lane must advance, player must survive,
  own guns stay quiet, travel must exceed3m. Quiet combat outcome is not forced.
- Contact: existing `winter_canyon_infantry`, full seed29 course. Tiger alone is
  placed on the first geometry-eligible authored25–42m route point near a real
  group. Ordinary desired lay/coax control; original rifle AI/perception responds.
  Own MG and defender discharge counters, actual contacts and live source IDs are
  required. Actual hostile contact/passage can drive score intensity; own coax
  does not synthesize a combat state.
- Blizzard: three ordinary T34s, base50m, weather1.8. The deterministic lane scan
  checks existing terrain/cover and the SAME local weather recognition contrast
  used by AI. If no three valid lanes exist, it fails before film rather than
  bypassing fog. Ordinary enemy reload/AI and Tiger lay/reload/firing remain.
  This is a readable heavy-snow fight, not a promise of a full whiteout bed;
  that bed only starts if actual local optical severity crosses its threshold.

Each scene has its own repeatable seed derived from the scene key. Only source
loading is synchronous/on-demand in capture; director, cue timing, source gain,
physical propagation, listener filtering and master limiter are unchanged.
Per-second `samples[].soundscape` and `soundscape_capture.final` record cue IDs,
lanes, sample cursors, director/program state, pending loads and failures.
`layered_sound_sources_advanced` requires at least two simultaneous voice cursors
advancing; this does not prove subjective audibility. Do not call a failed combat
receipt a firefight. `events` contain own/T34 discharge sequences, course defender
shots, resolved hits and listener KIA. `samples` retain camera, alive, position,
weather and actual machinery state. Audio receipt contains master limiter state.

Before showing: read every check and KIA time; decode MP4 fully with ffmpeg;
ffprobe resolution/fps/frame count/audio duration; inspect native first frame and
contact sheet; inspect peaks/nonfinite/clipped samples; listen to actual mixed
soundtrack. No replacement soundtrack or gain normalization.

Focused source tests for the next combined CPU build:
`combat_scene_suite::small_arms_review_tests`.

## Revised capture and ballistic findings

The first contact recording retained a failed hit-observation receipt. Its ordinary small-arms
projectiles were incorrectly retired when emitted partway through a 240 Hz tick. A fractional
birth step now consumes only the actual flight time; range retirement uses distance or the
full small-arms flight-time budget. Regression covers three birth phases through 120 m.

The first close-blizzard recording retained a failed own-fire receipt and a genuine fatal
ammunition detonation. Revised recording controls keep one live target rather than switching
every time another actor becomes nearer; the automated firing tolerance is 0.012 radians with
the ordinary gun drives. Initial T-34 staging now resets perception after the legacy staging
helper, so it does not inherit that helper's forced engaged state. No forced hit or survival.

The v1 recordings predate the completed sorted library import and cigarette/flashlight work.
New v2 recordings must use the rebuilt release and the 143-cue library; their actual
completion and acceptance must be read from their own receipts. These are AI/input-driven
simulation captures, not live frame-rate benchmarks or human playtests.
The completed contact v2 recording precedes the final three physical named-effect
bindings and revised packet/lighter poses. It can test its captured source snapshot,
but cannot demonstrate those later changes. Its exposed commander is killed at 0.7 s:
`listener_survived_coax_review` fails despite 221 coax rounds, 29 defender discharges
and real resolved combat outcomes. This is a retained damage diagnostic, unsuitable as
a sustained listening showcase. The next contact view is the existing
`winter_canyon_infantry_inside` fixture, with closed commander hatch and ordinary damage;
that planned view is not permission to assume survival or a passing result.
The night v2 recording uses the 143-cue build and does not exercise the later
contact- or held-prop-specific revisions; its source version must remain distinct.

At this documentation review, the completed v1 receipts show:

- `layered_winter_night_v1_20261002.json`: checks pass; no own or enemy tank shots,
  no course-defender discharges and no listener death were recorded.
- `layered_winter_contact_v1_20261002.json`: diagnostic. The MG discharged 221 rounds
  and course defenders discharged 50; `resolved_hit_observed` failed. Firing exchange
  is observed, but the clip does not validate the desired resolved-hit outcome.
- `layered_winter_blizzard_v1_20261002.json`: diagnostic. Both-sides-fired and own-shot
  listening checks failed; listener KIA occurred at 10.4333 seconds. It must not be
  presented as a successful two-sided firefight or a survival showcase.

The sorted-library compatibility issues are fixed in current source: score requirements
no longer collapse the eligible pool to the most specific filename, forced main-gun music
is rechecked when contexts change, and the exposed driver receives the exterior tag.
These changes do not retroactively modify or validate the v1 recordings. See
`docs/SORTED_AUDIO_LIBRARY_20261002.md` for import and runtime-binding boundaries.

## Review page receipt contracts

`tools/build_layered_ai_review.py` consumes `PREFIX.json` combat-case receipts, or an
`SCENE=all` `PREFIX_suite.json` that points to individual cases. It reads boolean `checks`,
`passed`, `video`, `soundtrack`, `contact_sheet`, firing counters and listener KIA. It keeps
completed failed cases as labelled diagnostics and refuses incomplete media or absent
required counters. It fully decodes the selected videos before creating the page. Run it
only once the requested completed artifacts exist; choose a new output if one already exists.

`tools/build_hand_item_review.py` is separate: ordinary mechanic capture writes
`PREFIX_manifest.json`, with `outcome_checks`, `required_timeline_actions_passed`,
`outcome_checks_passed`, `video`, `audio` and `poster`. It verifies native 800×450 at 24 fps,
exact declared video/sample frame counts and finite complete PCM audio, rather than
assuming the combat harness's 30 fps. The completed cigarette receipt passed the actual
parser/media check: 15/15 state checks plus required actions, 480 video frames and 882,000
stereo audio frames, finite raw peak 0.1891187, and no export gain adjustment. This check
created no review page and is not visual or listening acceptance. Flashlight completion
must be verified independently when its receipt and media are available.
The parent task's visual review rejected the cigarette v1 packet/lighter pose as too low
and dark; a later pose correction is in progress. The 15 passing state checks do not
override that visual finding, and the v1 media must not be presented as final hand-prop
acceptance. Page creation remains deferred until the parent selects the final captures.

Both pages use native pixel scaling, explicit playback controls and only one playing media
element at a time. A passing state receipt does not demonstrate smoke appearance, source
prop historical accuracy, per-effect audibility, sound balance or shadow quality. The hand
page explicitly discloses modern stylized props and approximate smoke transport. Registered
effect keys are not all bound to gameplay; consult the sorted-library document for the
five bound named controls, three bounded physical-source blends and three remaining
registration-only keys. Earlier captures cannot demonstrate the final physical blends
unless their binary/source provenance includes that completed binding pass.

## Completed accepted observation set

- `layered_winter_night_v2_20261002`:13/13checks,20seconds, quiet driving, no listener death.
- `layered_winter_contact_inside_v3_20261002`:18/18checks,30seconds,221coax rounds and29defender shots; closed commander hatch, real resolved contacts, no listener death.
- `layered_winter_blizzard_v2_20261002`:13/13checks,30seconds,2Tiger shots,3T34shots,3incoming and2outgoing hits,2enemy wrecks, no listener death.

All media fully decoded and source frames inspected. Review: `captures/layered_gameplay_review_20261003.html`. These films use the earlier143-enabled configuration; the final137-cue configuration routes previously inert musical phrases and explicitly defers6unprepared effects. The final death-chord-at-text change postdates these films; none contains a listener death. No listening approval is inferred from waveform/receipt checks.
