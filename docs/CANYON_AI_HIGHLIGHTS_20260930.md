# Canyon AI attempt recorder

`canyon-ai-run <capture-prefix> <seed> <maximum-seconds> [cuts.json]` is an explicit
offscreen test entry point. It constructs the same Gaea Canyon mode as the menu, with one
crewman, adaptive reinforcements, live return fire and normal damage. It never enables god
mode, stages casualties, teleports after deployment, replaces ammunition or suppresses fire.

The Tiger driver is a rule-based automated crew, not a trained or pixel-vision agent. It knows
the route printed on the field map. It acquires enemy tanks and antitank guns using geometric
line-of-sight queries within 420 m, without a camera-field-of-view or fog recognition model.
Aim points, target velocity and a ballistic solution are read from game state; mouse inputs
move the ordinary gun controls. This is a disclosed perception and aiming approximation.
It does not use the city harness's hidden-contact hunt, direct gun lay, automatic breech
service, repair-kit pickup or crew teleport shortcuts.

The crew drives from the open cupola, changes station through native actions, and reloads
through ordinary mouse/held-button and interior walking inputs. It walks around the breech,
opens the lever, extracts the casing, takes a rack round, seats it and closes the lever before
returning to the eyepiece. The native Q action leaves the gunner's optic. The current policy
has no field repair, extinguisher, infantry search or ammunition salvage routine.

An attempt ends on real Canyon completion or death, the explicit simulation-time limit, or
240 seconds without a new shot or at least 0.2% additional route progress. A control stall
is reported separately from a knockout. Generated telemetry, receipts and media live under
the flat repository `captures/` directory. A receipt records the seed, final state, outcome
and policy limitations. One-second telemetry is retained, including ammunition, reloads,
shots, damage, casualties, living/total T-34s and route progress.

The optional JSON cut list contains sorted, disjoint objects with `start_s`, `end_s` and
`label`. Recording runs the full simulation from the start, including omitted intervals,
and renders selected moments through the real first-person gameplay camera at 30 fps.
Sound is the game's own capture mix, advanced through the omitted intervals too. It is
not a cinematic camera or an animation of supplied outcomes. Probe and recording results
must be compared before presenting a recording as the replay of a particular probe.

Control-development probes are retained as such. The first exposed a blocked straight-line
walk through the breech; the next proved repeated reloads but fired with an overly broad
angular tolerance for a distant field gun. Only the test driver's walking and trigger
discipline were adjusted; the game and enemy combat rules were not changed.

## Three seeded attempts

The corrected policy was given seeds 29, 73 and 101, with a two-hour ceiling each. None
completed the course. These results evaluate this particular privileged, rule-based crew;
they do not establish a human win rate or a trained policy's performance.

| Seed | End time | Route | T-34 kills / total fielded | AT guns silenced | Result |
| --- | --- | --- | --- | --- | --- |
| 29 | 14:32 | 64.8% | 2 / 10 | 2 | Navigation stalled; Tiger alive |
| 73 | 24:44 | 91.1% | 12 / 12 | 3 | Navigation stalled; Tiger alive |
| 101 | 08:36 | 48.3% | 2 / 6 | 2 | Ammunition detonation; run lost |

Seed 73 completed 15 physical reloads and received six non-penetrating hits, leaving its
radio damaged. Its last T-34 kill was at 19:37. Seed 101 received one penetrating hit that
ended the run. Reinforcements were admitted by the live director, without forced arrivals.
This does not validate the 60–120-minute balance target. Route recovery around obstacles
and post-combat continuation are clear next AI work; the stalled cases were not teleported
past their obstacles to manufacture a completion.

The highlights use the shader renderer at 640×360 internally, encoded at 1920×1080/30 fps,
matching the earlier Canyon gallery's renderer. The physical-renderer recording was stopped
and retained as an explicitly named partial capture. Both full simulation probes and the
finished render outcomes are retained for comparison. Editorial captions are outside gameplay.
Both recorded attempts have exactly the same final telemetry and outcome as their full
simulation probes, despite using the alternate renderer and advancing the audio mixer.
The fatal shot receives a three-second frozen-world/audio tail, as the native death presentation
allows already-emitted sound to arrive; it does not advance a dead crew or invent an explosion.

Release compilation, new-module formatting and `git diff --check` passed. The unit suite
passed 1,404 tests with 54 ignored; ten local-socket tests were blocked by the filesystem/network
sandbox, and all ten passed when rerun with host loopback access. The duration guard rejects
non-finite input before building a GPU scene. These are automated checks, not a claim of new
human/native-window acceptance. The pre-existing repository-wide formatting debt remains.
