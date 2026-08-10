# AI battle lab and showreel

The battle lab runs the real armor, ballistics, crew workload, fire, and damage simulation. Neural
policies propose movement and trigger intent; they do not bypass station changes, reloads, gun lay,
or penetration rules.

## Current proved result

The 2026-07-23 nine-seed gauntlet tested the evolved Tiger against one through six evolved T-34s,
with a 90-second cap:

| T-34s | T-34 wins | Tiger wins |
|---:|---:|---:|
| 1 | 0/9 | 9/9 |
| 2 | 0/9 | 9/9 |
| 3 | 0/9 | 9/9 |
| 4 | 3/9 | 6/9 |
| 5 | 5/9 | 4/9 |
| 6 | 8/9 | 1/9 |

The empirical majority threshold is therefore **five T-34s** for these policies, seeds, arena, and
time cap. This is a measured balance result, not a historical claim. Raw results are in
`artifacts/ai-gauntlet-2026-07-23/gauntlet.csv`.

## Commands

All commands require a Vulkan GPU. `ffmpeg` is also required for video.

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
cargo run --release -- ai-showreel artifacts/ai-showreel \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin auto 90 0

# Record one fixed force size and seed (useful for checkpoint comparisons):
# outdir t34_weights tiger_weights count seed match_seconds act
cargo run --release -- ai-take artifacts/ai-take \
  assets/ai/weights_academy.bin assets/ai/weights_tiger.bin 5 0x34a0000d 90 0
```

Set `KESSEL_SHOWREEL_4K=1` for a 3840×2160 output. The default is 1920×1080 at 24 fps, H.264 CRF
14. The camera director cuts among establishing, Tiger-shoulder, T-34 chase, crossfire,
ground-orbit, tactical, and event-close shots, with smooth motion inside each shot.

Generated PNG and MP4 media are intentionally ignored by Git; run metadata and CSV balance evidence
may be committed without bloating repository history.

## Progression cut

The current 30-second learning film holds the T-34 count and seed fixed and changes only the Tiger
checkpoint:

- generation 0: Tiger destroyed; four T-34s survive;
- generation 13: Tiger survives and destroys all five;
- generation 18: Tiger survives and destroys all five, with the strongest held-out exam score.

The rendered deliverables live locally under:

- `artifacts/ai-showreel-2026-07-23/`
- `artifacts/ai-progression-2026-07-23/`

## Scaling to dozens or 100 tanks

The present lab intentionally uses the six full-detail campaign tank rigs. A 100-tank battle should
not be made by cloning those rigs. It needs a second simulation tier:

1. pooled/instanced exterior rigs and LODs;
2. batched policy inference at a lower decision rate;
3. spatial partitioning for target selection, line of sight, and shell queries;
4. simplified off-camera crew/interior state, promoted to full fidelity near the camera;
5. deterministic replay logs so one simulation can be re-filmed from many angles.

That is the next prerequisite for the armada film.
