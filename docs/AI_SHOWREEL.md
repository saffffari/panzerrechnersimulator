# AI battle lab and showreel

The battle lab runs the real armor, ballistics, crew workload, fire, and damage simulation. Neural
policies propose movement and trigger intent; they do not bypass station changes, reloads, gun lay,
or penetration rules.

## Current proved result

The 2026-08-27 nine-seed gauntlet tested Hal's generation-101 Tiger policy against one through six
evolved T-34s in the current gameplay worktree, with a 90-second cap:

| T-34s | T-34 wins | Tiger wins |
|---:|---:|---:|
| 1 | 0/9 | 9/9 |
| 2 | 0/9 | 9/9 |
| 3 | 1/9 | 8/9 |
| 4 | 2/9 | 7/9 |
| 5 | 2/9 | 7/9 |
| 6 | 5/9 | 4/9 |

The empirical majority threshold is therefore **six T-34s** for these policies, seeds, arena, and
time cap. This is a measured balance result, not a historical claim. The previous generation-18
policy scored 9/9, 9/9, 8/9, 5/9, 4/9 and 2/9 in the identical current-code gauntlet. Raw A/B
results and the recovered Hal training history are in `artifacts/hal-training/20260826/`.

## Commands

Commands use Metal on macOS or Vulkan on Linux. `ffmpeg` is also required for video.

```sh
# Evolve/resume the Tiger policy:
# generations population match_seconds wall_clock_minutes t34_weights tag mutation_sigma
cargo run --release -- train-tiger 80 16 90 0 \
  assets/ai/weights_academy.bin tiger 0.12

# Measure 1–6 T-34s:
# outdir t34_weights tiger_weights seed_count match_seconds act
cargo run --release -- ai-gauntlet artifacts/ai-gauntlet \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin 9 90 0

# Sweep, choose the majority-threshold battle, and film it:
# outdir t34_weights tiger_weights auto|count match_seconds act
cargo run --release -- ai-showreel captures \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin auto 90 0

# Record one fixed force size and seed (useful for checkpoint comparisons):
# outdir t34_weights tiger_weights count seed match_seconds act
cargo run --release -- ai-take captures \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin 5 0x34a0000d 90 0

# Record the authored Act-I village escalation: 1, then 2, then 6 T-34s.
cargo run --release -- ai-campaign-wave-take captures \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin 150

# A genuinely different finale can select a new deterministic seed and the encirclement geometry:
KESSEL_SHOWREEL_FINAL_SEED=0x34a00014 \
KESSEL_SHOWREEL_FINAL_LAYOUT=ring \
KESSEL_SHOWREEL_FINAL_GEOMETRY_M=112 \
cargo run --release -- ai-campaign-wave-take captures \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin 150

# Prove the same four-body sequence without encoding/submitting any video frames.
# t34_weights tiger_weights seconds final_seed crossfire|ring geometry_metres
cargo run --release -- ai-campaign-wave-preflight \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin 150 \
  0x34a00002 crossfire 96
```

The campaign-wave take is acceptance-gated: all three waves must start, the Tiger must die in wave
three, at least two separated impacts must be visible from the interior, and the fatal event must
remain inside for at least two seconds. It stages four authoritative couch crew bodies—Commander,
Driver, Gunner, and Loader—with the radio/MG post empty, so KIA bodies and takeover vacancies come
from the live crew model. Its current soundtrack is a deterministic spatial film mix assembled from
simulation events; it is not a capture of the runtime audio sink.

Set `KESSEL_SHOWREEL_4K=1` for a 3840×2160 output. The default is 1920×1080 at 24 fps, H.264 CRF
14. The camera director cuts among establishing, Tiger-shoulder, T-34 chase, crossfire,
ground-orbit, tactical, and event-close shots, with smooth motion inside each shot.

Generated PNG and MP4 media go into the flat `captures/` directory and are intentionally ignored by
Git. Seed/layout-bearing filenames keep distinct takes from colliding; run metadata and CSV balance
evidence may be committed without bloating repository history.

## Progression cut

The current 30-second learning film holds the T-34 count and seed fixed and changes only the Tiger
checkpoint:

- generation 0: Tiger destroyed; four T-34s survive;
- generation 13: Tiger survives and destroys all five;
- generation 18: Tiger survives and destroys all five, with the strongest held-out exam score.

The historical originals remain locally under:

- `artifacts/ai-showreel-2026-07-23/`
- `artifacts/ai-progression-2026-07-23/`

Flat, path-labelled copies of those renders are also available in `captures/`.

## Scaling to dozens or 100 tanks

The present lab intentionally uses the six full-detail campaign tank rigs. A 100-tank battle should
not be made by cloning those rigs. It needs a second simulation tier:

1. pooled/instanced exterior rigs and LODs;
2. batched policy inference at a lower decision rate;
3. spatial partitioning for target selection, line of sight, and shell queries;
4. simplified off-camera crew/interior state, promoted to full fidelity near the camera;
5. deterministic replay logs so one simulation can be re-filmed from many angles.

That is the next prerequisite for the armada film.
