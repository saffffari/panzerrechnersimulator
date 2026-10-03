# Named physical effect bindings

This bounded implementation uses the user's final sorted `panzer_sounds/effects` assignments through the imported library's explicit event IDs. Filename labels establish intended event use; no claim is made that the recordings are measurements of a Tiger or have been auditioned here.

Implemented routes:

| Named event | Actual trigger | Existing behavior retained |
|---|---|---|
| cigarette_pickup | E successfully collects a reachable, uncollected world cigarette packet | Whole-packet capacity check; world object removed once; Pickup fallback |
| hammer_pickup | Physical repair toolbox/hammer is taken with free hands | Tool ownership and source pickup bearing; Pickup fallback |
| trigger_pull_not_loaded | Blocked/misfiring main gun or failed actual MG trigger rising edge | Validity/cadence/crew gates; no duplicate click over a loaded discharge; control fallback |
| jammed_traverse | A functional powered traverse is actually commanded against a blocked ring | One finite recording per attempted movement, rearmed only by release; existing procedural loop if recording unavailable |
| jammed_gun_elevation | Actual laid elevation demand is blocked by the existing gun-mechanism gate | One finite recording per attempt, no sound simply because the module is damaged; finite control fallback |
| fire_igniting | Existing first non-catastrophic fuel/engine ignition transition | Low initial character inside the existing 0.55-second physical ignition; one scheduled source; existing propagation/emission |
| impact_stopped | Authoritative cannon Bounce or Shatter on enemy, ally or player armor | Existing physical clang/own-hull response plus restrained character; Penetration and HeBlast excluded |
| light_round_impact | Actual small-arms Vehicle armor contact, not body/ground/range retirement | Existing bullet/panel source, structural arrival or exterior wavefront, actual caliber/velocity/source position retained |

`audio_named_effects` decodes once at Audio construction: at most 8MiB of mono PCM,12 seconds per clip and 4 variants per event. Only explicit allowlisted event names whose cue role is finite `effect` are accepted; melodic aliases and looping cues are rejected. Original PCM is collapsed to a positioned mono source. Physical control levels, headset transfer, bearing, current hearing and the common master remain authoritative. Authored music cue gains are not reused. Recorded tails are retained and controls add no second synthetic room tail. Startup metadata reports loaded content IDs/frame counts/rejections; `named_effect_snapshot` additionally exposes actual source selections.

The remaining three sources now use intentionally small character components rather than complete blast replacements. Local PCM measurements show the 9.999-second fire file has strong activity around 0.04–1.24seconds and a long quiet background tail; stopped-impact files include tonal 1.44/1.88second bodies; light-round impact is 0.592 seconds with its dominant contact in the first 0.12 seconds. These are signal measurements, not listening or authenticity verification. Source recordings are untouched.

Startup preparation finds the first meaningful onset and keeps at most 0.40 seconds for fire,0.45 seconds for stopped armor, and 0.14 seconds for light armor. A 25Hz DC-removal filter and short onset/end tapers avoid stored offsets and splice clicks. The character is bounded respectively to 18%,12%,22% of the original physical voice's peak before matching that voice's original integrated energy. The original waveform length and onset remain; a shorter original support receives an additional end taper. There is one composite voice, never a second full-volume blast. Existing at-ear/source calibration and subsequent spectral propagation/headset path remain authoritative. The physical pressure/panel response stays primary; these blends are authored choices requiring listening acceptance.

The fire file matches the prior Match_Stick source by content identity, but now explicitly carries the user's `fire_igniting` label. Only its initial catch is used for the real ignition event. It is not automatically bound to the handheld lighter. There is no distinct lighter clip in the sorted source list.

Deliberately unused: generic `light_impact` is not stacked over the specific `light_round_impact` for the same contact; `error` has no reviewed UI failure-event mapping yet; `surprise_contact` is not a hidden-enemy alarm and has no gameplay proximity trigger. These registry entries remain available without fabricated events.

Meaningful pending integrated CPU filters: `audio::named_effects::tests` (six tests: explicit admission/variant order, cache bounds, contact energy/onset/support, invalid/silent safety, finite excerpt bounds, and exclusion of penetration/HE blast), `audio::feedback::tests` (existing trigger/movement lifecycle), and the hand-item tests. No listening or gameplay acceptance is claimed from source parsing or metadata alone.
