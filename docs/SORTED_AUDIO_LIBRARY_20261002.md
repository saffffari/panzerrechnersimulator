# Sorted audio library integration — completed 2026-10-03

**Final routing:** 137 enabled cues out of 151 unique recordings (158 source files).
All ten active effect recordings have actual gameplay routes through eight named keys.
Twenty-three former inactive effects now use finite foreground score phrases through the
existing programme; six unprepared/unrepresented effects are explicitly disabled. Alongside
six unassigned scene recordings and two `unused` recordings, 14 identities are inactive.
The final manifest/configuration, exact preceding four-file backups, verification and
36-test receipt are in `artifacts/runtime/sorted_audio_routing_final_20261003/`.

No original or playback PCM changed. All 143 previously imported derivatives remain intact;
the disabled recordings remain available for later authoring. These enabled counts describe
implemented eligibility/routes, not a guarantee that every cue plays in a particular run
or that its sound has passed subjective listening acceptance.

## Final finite-phrase routing

- Nine production-labelled synth, bass and melodic one-shots use `music`; nine percussion
  one-shots use `combat`; one dub FX hit uses `threat`.
- The three `combat/surprise_contact` variants use finite `combat` phrases, with a required
  factual combat context. Their earlier executable `surprise_contact` event is removed;
  they never signal an unseen approaching enemy.
- `music/PHOTAY_texture_salt_mine_riser_02_D.wav` stays in its current `music` folder role
  and plays as a finite phrase. It does not inherit another recording's measured riser peak.

These 23 assignments use the existing single foreground score lane, seeded programme,
complete-phrase endings, rests and compatibility checks. They add no random effect scheduler
or incompatible background overlay. All prior gain and rack settings for the 23 identities
were verified exactly unchanged. Unknown key/mode/BPM values remain unknown. The authored
mapping is explicit in `tools/soundscape_routing.py`; source folder/path provenance remains
in the manifest instead of moving or renaming the recordings.

The six explicitly deferred recordings are snow footsteps (requires segmenting the real
multi-step recording and binding actual snow foot contacts), redwood campfire (requires a
spatial burning-source binding), coin box (no represented interaction), two error sounds
(no authored failure action), and generic `light_impact` (not stacked over the implemented
`light_round_impact`). Their reasons are in `soundscape.json.deferred_effects`. The error and
generic-impact names remain in the descriptive named registry for later authoring, but
their IDs are absent from executable events and active cues.

Final active roles: music 22, combat 29, threat 12, explore 9, driving 1, intro 11,
stinger 12, cabin 16, night 6, day 6, blizzard 2, engine hiss 1, effect 10.
The final 36 Python tests (21 catalog, 15 policy/merge) and additional promotion checks
verify all eight bound keys, no active unrouted effect clips, 23 finite phrases, six
deferred effects, no hidden-contact event, preserved gain/racks and unchanged derivatives.
No Rust build or GPU capture is claimed by these Python/configuration checks.

## Prior import and audit evidence

The following records the earlier 143-entry import and compatibility audit. Its counts and
inactive-effect findings are historical; the final routing above supersedes them.

The completed sorted-source scan has **158 files, 151 unique content identities and 143
enabled runtime cues**. Seven duplicate files remain aliases without extra selection weight.
Six unassigned scene recordings and two files in `unused` remain inactive. This supersedes
the earlier partial 73-cue runtime snapshot and the earlier 155-asset manifest.

The full scan was already complete in
`artifacts/runtime/sorted_audio_scan_20261002.json`; this pass reused it without rescanning
the source tree. Source originals were not edited. All 73 existing playback derivatives
were verified byte-for-byte unchanged; 70 newly enabled identities received 44.1 kHz
stereo PCM16 derivatives without baked gain or trimming. Enabled derivatives total
596,476,600 bytes. The largest decoded float32 clip remains below the runtime's 64 MiB
per-clip admission limit; the library is still decoded on demand, not all loaded at once.

The live manifest, numerical cue policy, generated binding proposal and merged runtime
configuration were promoted only after every enabled identity had verified playback and
every event resolved to an enabled cue. Exact preceding JSON bytes are retained in
`artifacts/runtime/sorted_audio_stage_20261002/backup/`. The sibling `receipt.json` records
hashes, inventory, new playback identities, checks and promotion time; `merge_receipt.json`
records the three-way merge. No unresolved runtime cue remains.

## Selection and authoring

Current folder and explicit filename intent controls routing. Combat-labelled music joins
only the combat role. Driving-labelled music is restricted to driving; a combined
`calm_driving` label permits both exploration and driving. `night`, `winter` and
`inside_tank` labels add the respective existing game context requirements. A named musical
riser is registered as an effect rather than being selected as an arbitrary calm melody.
`lonelytank` and `layer` are recorded descriptive words, not invented game conditions.

The subsequent runtime compatibility review found and corrected three integration issues:

- Foreground score context requirements now filter eligibility without excluding the more
  general eligible material. Before that fix, the maximum-specificity rule reduced inside
  combat from 17 candidates to one, night exploration from nine to one, night suspense
  from 12 to one, and night music from 13 to two. These full eligible pools now reach the
  selector. Harmonic/tempo-compatible subsets may still be preferred during a transition;
  these counts are not a promise of equal audible frequency across all situations.
- A forced main-gun successor is revalidated against the current requested role and context,
  even when its programme token has not changed. A cached night/inside cue cannot bypass
  a subsequent day/exterior requirement change.
- Game input uses the existing station-specific `buttoned_up` predicate for the `inside`
  tag, including the exposed driver hatch position.

All current required tags (`night`, `driving`, `combat`, `inside`, `winter`, `spring`) are
supplied by the game/runtime. This resolves the three findings in the earlier read-only
`sorted_audio_runtime_compatibility_20261002.json` snapshot; that receipt is retained as
the pre-fix evidence. Score selection still follows the nonliteral programme and factual
hostile-contact gate, not proximity to unseen enemies. Environmental/scene specificity and
the fixed per-night choice remain separate policies.

The merge preserves authored gain, rack, fades and transition timing when they differ from
the prior proposal. Current source routing overrides obsolete aliases, and absent,
unassigned or unused identities cannot remain active through an old configuration. This
particular snapshot had no live per-cue overrides beyond its prior generated proposal and
no active cues to retire; the stale empty sunset event was removed. Five absent identities
were removed from the authoritative manifest; their earlier metadata remains in the backup.

Filename BPM/key labels survive renames only when the complete source SHA-256 matches.
Missing keys stay unknown; bare tonic labels never imply major/minor. No pitch, tempo or
historical semantic inference was added. `_loop` retains explicit complete-file repeat
intent. Named sunrise, sunset and dusk events remain finite full-source exceptions, so a
`sunrise_loop` recording is not truncated but does not play forever after a solar crossing.
The approved intensity-transition riser retains its 5.404943-second source peak and
four-second preferred lead. Incoming-hit riser events remain absent.

## Named effect registry

These keys are available in both `events` and `named_effect_registry` in the runtime
configuration. They only register user-labelled content; this import does not call them
from gameplay or substitute them for physical firing and propagation.

| Event key | Unique variants |
| --- | ---: |
| `cigarette_pickup` | 1 |
| `hammer_pickup` | 2 |
| `fire_igniting` | 1 |
| `jammed_gun_elevation` | 1 |
| `jammed_traverse` | 1 |
| `trigger_pull_not_loaded` | 1 |
| `impact_stopped` | 2 |
| `light_impact` | 1 |
| `light_round_impact` | 1 |
| `error` | 2 |
| `surprise_contact` | 3 |

`stopped_impact` is an explicit filename spelling alias for `impact_stopped`. No recording
was labelled specifically as a lighter mechanism. The broader `fire_igniting` recording
therefore has not automatically been assigned to that action.

A later runtime binding pass calls five names on actual control actions:
`cigarette_pickup`, `hammer_pickup`, `trigger_pull_not_loaded`, `jammed_gun_elevation`
and `jammed_traverse`. The named control path retains existing ear/headset/bearing
processing and uses the fallback when its sample is unavailable; it does not add a second
copy on top of the fallback.

The final physical binding pass also uses three bounded onset excerpts within an existing
physical source: `fire_igniting` uses an 18% base-peak blend ratio over 0.40 seconds
at the first fuel ignition; `impact_stopped` uses 12% over 0.45 seconds for
actual bounce/shatter outcomes, including the own tank, excluding penetration and HE;
`light_round_impact` uses 22% over 0.14 seconds for resolved small-arms vehicle
armor contacts before their existing structural/world paths. Each is one composite source
retaining the base integrated energy, onset and buffer duration, without an extra event
or room rack. Source originals stay unchanged; cached excerpts use 25 Hz DC cleanup and
short edge tapers. The ratios apply before final energy matching. This is an authored timbre contribution, not a measured Tiger recording
or proof of acoustic realism. Six focused regression tests were added by the binding task;
their compiled result belongs to the parent-controlled build, not the import's 33 Python tests.

`light_impact`, `error` and `surprise_contact` remain registration only. Generic impact
is not stacked over the precise small-arms impact, and no hidden-enemy alarm is triggered.
Loading or registering a sample does not prove it has played. Eight of the eleven named
keys now have actual gameplay bindings; native mixed listening acceptance is still separate.

The broader `effect` role contains **39 enabled recordings**, not just these eleven named
keys. Actual gameplay bindings cover **10 unique recordings** through the eight bound
keys. The three uncalled named keys above cover another six recordings. The remaining
**23 recordings have neither a named event membership nor an automatic runtime route**.
Examples include `OS_ORG_Footsteps_In_The_Snow`, `PHOTAY_texture_redwood_campfire`, the
additional `PHOTAY_texture_salt_mine_riser_02_D`, and generic bass, kick, snare and melodic
one-shots. Their descriptive filenames are not implemented physical-event bindings.

All 39 have finite playback policy and only the `effect` role alias; none of the 23
unassigned recordings is automatically mixed into suspense/combat or picked by a generic
random one-shot scheduler. The `effect_registry` lists available content, not executable
triggers. Consequently **29 of the 39 effect recordings are currently inactive during
ordinary gameplay**. “143 enabled cues” describes imported catalog/configuration entries;
it does not claim that every recording can presently be triggered or has been heard.
Additional authorship is required to use those 23 recordings; this final audit adds no
speculative footsteps, impacts, enemy alarms or automatic effects to the launch build.

## Verification boundary

The catalog and policy suites passed **33 tests: 20 catalog and 13 policy/merge tests**.
They exercise same-content metadata retention, unused precedence, explicit filename
contexts, loop/full-phrase handling, finite solar events, exact named registration and
authoritative retirement while preserving authored treatment. Import checks verified the
143 playback identities, untouched existing derivatives, coherent JSON hashes and
resolving events. Test receipt: `artifacts/runtime/sorted_audio_stage_20261002/tests.json`.

This is completed import/configuration work, not listening acceptance. Numerical mix,
fade and ending-rack suggestions remain provisional. No GPU/game run or Rust build was
performed by this import task; the main task owns actual layered gameplay capture and
audible validation.

The ordinary soundscape budget remains 12 active/fading voices, four pending decoder
requests and a 256 MiB playback cache. After a day/night cycle, day, night, cabin and
foreground score normally account for four voices; blizzard and failed-engine hiss can
raise this to six, while muted beds may retain their voice. Transitions and finite events
use the remaining space; admission can wait when all 12 slots are occupied. The six death
cues occupy 29,586,568 decoded bytes when prewarmed. The largest clip is 32,755,016 decoded
bytes. These are source/budget checks, not measured interactive CPU cost, total process
memory or audible balance. Six unassigned scenes remain disabled, and this import does
not establish location-specific scene playback or historical authenticity of recordings.

For a later completed full scan, use a fresh staging directory:

```sh
OPENBLAS_NUM_THREADS=1 python tools/stage_sorted_audio_import.py \
  --scan artifacts/runtime/sorted_audio_scan_20261002.json \
  --stage artifacts/runtime/sorted_audio_stage_NEW
```

Add `--promote` only when that completed sorted snapshot is the intended active library.
The tool refuses to overwrite a staging directory and checks that live JSON did not change
during staging before preparing replacements. Exact prior bytes are backed up first.
