# October 1 AI recovery

Worktree: `canyon-v2-20260930`, based on `b7c76fa`. Claude's uncommitted
`bluewinter-20261001/src/canyon_pilot.rs` recovery was imported and refined here;
the original worktree was preserved.

## Training memory failure

The host kernel recorded an OOM kill at 02:04:39 on October 1. The failed
`night1001` run produced no checkpoint. Render-free `Game::update` still writes
instance/vertex data to the wgpu queue. Those writes allocate staging memory;
without a submission and completed poll, the allocations accumulate. Polling
alone does not submit them (wgpu 27's `Queue::write_buffer` contract).

Training and Canyon evaluation now submit and wait every eight simulation steps,
and drain at episode/end boundaries. This submits uploads only; it does not
render a scene or change physics, combat, perception, or policy inputs. Training
also defaults unused RT pipelines off; an explicit `KESSEL_NO_RT` override wins.

Matched RTX 4090 test: one generation, population 3, six-second episodes, arena,
self-adaptive sigma, `KESSEL_NO_RT=1` for both builds:

| Measurement | Original | Fixed |
| --- | ---: | ---: |
| Peak RSS, KiB (`/usr/bin/time -v`) | 20,912,832 | 3,468,360 |
| Champion exam fitness | 138.217 | 138.217 |
| Reloaded exam fitness | 138.217 | 138.217 |

The two champion binaries are byte-identical (SHA-256
`8ea7efe9632c57698f2fc48f00956dd220a06e0c049ba52ad7e9782b4e302789`).
Logs: `artifacts/runtime/train_audit1001_{baseline_nort,fixed}.log`.
An earlier RT-enabled diagnostic timed out during pipeline compilation and is
excluded from this comparison. These are memory measurements, not a GPU speed benchmark.

## Deterministic recovery and promotion decision

A second defect was exposed by replay: `reset_match` retained mutated world state between
training episodes. Each seed now constructs an independent Game/Renderer on the shared device,
then drains pending GPU uploads before disposal. The final champion must reproduce its exam
score exactly when reloaded, or training returns an error.

The bounded recovery completed two generations, population four, 30-second episodes on both
maps. Peak RSS was 9,327,904 KiB; the champion's exam and reloaded score were both 116.500.
This is a completed recovery check, not the interrupted 2,000-generation overnight experiment.

Independent promotion evaluation used ten 90-second episodes (five fresh seeds on each of two
maps) and repeated every episode. Both models replayed bit-exactly. The active academy baseline
scored **114.2505**; the recovery candidate scored **111.0755**. **Do not promote the candidate.**
`weights_academy.bin` and the separate Tiger policy remain unchanged. These evaluations used the
preserved `artifacts/runtime/panzer-ai-isolated1001` executable before later visual/avoidance edits.

## Foot policies

The 512-epoch files were reproduced byte-for-byte in the separate directory
`assets/ai/foot_candidates/recovery1001`. Comparison against the saved 128-epoch
baselines used 128 episodes per role and fresh seed `0xF007_E7A2_2026_1001`.

| Role | Baseline agreement | Candidate agreement | Baseline MSE | Candidate MSE |
| --- | ---: | ---: | ---: | ---: |
| Tiger crew | 94.66% | 97.14% | 0.008109 | 0.002549 |
| Soviet rifle | 90.82% | 97.46% | 0.007882 | 0.002420 |

All four evaluations had zero illegal actions and 100% **micro-curriculum**
success. This is an imitation evaluation, not a measured campaign win rate.
The improved active foot weights and the two preserved baselines are small,
engine-generated files now explicitly included in source control.

`train-foot-ai [epochs] [output-directory]` now defaults to
`assets/ai/foot_candidates/epochs_<epochs>` and refuses either existing output
before writing anything. It no longer replaces the active policies by default.
`eval-foot-ai output.json tiger-baseline tiger-candidate soviet-baseline soviet-candidate`
writes the comparative receipt.

## Canyon driver

This is the explicit offscreen crew-input test driver, not an autopilot installed
in human play. Route-progress recovery alternates sides, escalates its reverse
and lateral detour, and pauses its watchdog through combat/reloads. The refined
driver brakes before large heading changes, checks blockages sooner, and gives
the obstacle more clearance. It uses normal inputs and retains manual reloads,
enemy return fire, terrain contacts, and damage. No collision or damage bypass
was added.

The long-course receipts below precede the final near-field avoidance guard. That guard brakes
before contact, removes residual forward throttle into a blocked hull, and permits reverse only
with rear clearance. It applies to AI commands, including wreck obstacles, without changing
human input or collision geometry. Its regression advances the actual Tank track dynamics past
a parked T-34 rather than just checking a steering scalar.

The intermediate seed-137 regression passed the old 64–65% blockage and reached
83.75%, with 10 kills, before a disabled track and turret traverse left it stalled.
This proves passage through the reported navigation failure; it is not a full
course victory. Final-run receipts and the gameplay clip are recorded in the
machine-readable recovery report.

## Validation and operation

The full host unit suite passed: 1,444 passed, 55 ignored, zero failed. An initial
sandbox run had ten loopback-socket permission failures; the host rerun passed
those tests. Two CLI regressions verify rejected training inputs allocate no GPU
and foot training refuses existing outputs without writing a partial pair.

Training tags must be simple names and cannot overwrite existing candidate files.
The wall cap is checked between candidates; an unfinished generation is discarded
and the last fully evaluated champion is retained. The final champion must pass
reload determinism or the command fails. Candidate progress is logged.

`eval-tank-ai seconds maps weights [weights ...]` evaluates and repeats each
candidate using promotion seeds disjoint from the champion-selection exam seeds.
Compare T-34 candidates with `weights_academy.bin`; `weights_tiger.bin` is a
different role and must not be replaced by the T-34 trainer's output.

Recovery run and final promotion decision: see `catalog/ai/recovery_20261001.json`.
Raw logs, traces and video stay in the ignored runtime/capture directories; the
tracked receipt records their hashes and measured results.
