# Editor frame costs — 5 November 2026

The 60 Hz deadline is 16.67 ms. The current runtime has count/step limits, not an enforced per-system wall-time allocation. CPU and GPU can overlap; the existing live log measures CPU simulation and render preparation/submission, not isolated GPU execution.

## Measured stable editor session

Source: `/home/alex/panzer/editor-stable/frames-20261105-1211.jsonl` (2,971 frames, 2,400 with simulation updates). Full statistics: `target/mantlet-world-20261105/frame-cost-audit.json`.

| CPU phase | Quiet frames under 16.67 ms combined, mean | Frames above 33.33 ms combined, mean |
|---|---:|---:|
| Simulation | 4.75 ms | 118.06 ms |
| Render preparation/submission | 6.09 ms | 22.25 ms |
| Combined | 10.84 ms | 140.31 ms |

These groups contain 2,007 and 243 frames respectively. Slow-frame simulation includes 56.02 ms interior fire (46.18 ms smoke-clearance is a subset), 25.09 ms blood (6.13 ms surface update is a subset), 4.34 ms penetration effects, and other work. Inclusive probes must not be added to their parents. Worst observed simulation is 368.86 ms; simulation p95 over all active frames is 121.68 ms. Reset/settling frames can include 60+ updates; ordinary impact catch-up repeatedly reached six updates per presented frame.

## Live versus offline

Live starts at 960×540 internal pixels, gameplay transport, `KESSEL_REAL_STEPS=7,7,4`. Interactive material simulation selects a 1,500-particle plate budget and 3 projectile cells across, versus a 6,000-particle budget and 8 cells across for reference capture. These are discretization limits, not time limits. Tissue integration also has a separate interactive schedule.

The new sequential impact path advances elapsed plate/projectile microsteps on the caller thread. It does not compute future contacts, but it can monopolize the frame. Added crew collision sweeps and newly exercised fire/blast effects also run live. The existing 24,576 interior soot-particle limit was not an accidental switch to reference resolution; new impacts make that expensive existing path active more often.

RANGE previously bypassed the game's presentation batching and rebuilt particles during intermediate catch-up states. The current candidate shares publication after catch-up, preserves collision/damage steps, batches independent crew contact queries, and shares posed crew geometry between blood and sparks within an update. The interactive window now explicitly selects interactive material quality, so inherited reference-capture environment settings cannot change it.

## Validation boundary

Native synchronized offscreen readback is slower than a presented window and is not a live FPS measurement. Before the batching work, the same 120-frame BR-365 strike measured 78.4 ms median / 104.0 ms p95; initial exact-query/integration optimizations only reached 75.1 / 99.9 ms. Those first changes did not resolve the regression. A 60 FPS claim requires live post-impact tail latency to meet the deadline; passing collision and temporal-invariance tests is not that proof.
