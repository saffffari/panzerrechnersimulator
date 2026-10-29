### artifacts/runtime/session-20261028-225509.jsonl

Session log `artifacts/runtime/session-20261028-225509.jsonl`: 298 one-second lines, 297 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 17764 frames over 297 s.

| metric | value |
|---|---|
| mean frame time (exact) | 16.75 ms (59.7 fps) |
| median second fps / 5th-percentile second fps | 59.2 / 56.5 |
| p50 of per-second p50 | 16.65 ms |
| p95 of per-second p95 | 23.00 ms |
| p99 of per-second p99 | 34.74 ms |
| worst single frame | 75.6 ms |
| seconds with p99 > 33.3 ms | 5 |
| seconds with p50 > 16.7 ms | 36 |
| CPU simulation mean / p99-of-p99 | 4.09 / 20.34 ms |
| CPU render submission mean / p99-of-p99 | 9.36 / 17.17 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 3.30 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.8 | 16.66 | 22.61 | 33.32 | 75.6 | 2.85 | 8.97 | 0.58 | 25 |
| 1 | 60.0 | 16.66 | 20.86 | 25.53 | 48.1 | 2.93 | 9.24 | 0.58 | 59 |
| 2 | 59.7 | 16.65 | 22.09 | 31.92 | 71.4 | 4.30 | 8.97 | 0.58 | 60 |
| 3 | 59.7 | 16.61 | 23.35 | 30.05 | 36.5 | 4.87 | 10.05 | 0.58 | 59 |
| 4 | 59.3 | 16.60 | 23.33 | 34.74 | 39.2 | 5.52 | 9.56 | 0.49 | 59 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 128 s | 51.7 | 18.8 / 45.0 / 71.4 | 4.82 / 14.99 | 9.66 / 11.57 | 19.48 | enemy 394 m; 8 live AFV; rm events 3, sparks 2, parcels 249, lights 10; lights offered 9; storm 0.58; station Loader; beat 3 'The gun line at the fork' |
| 1 s | 56.7 | 18.1 / 43.9 / 75.6 | 4.00 / 12.15 | 8.77 / 11.70 | — | enemy 651 m; 11 live AFV; rm events 0, sparks 0, parcels 22, lights 10; lights offered 10; storm 0.58; station Commander; beat 1 'Read the ground' |
| 244 s | 51.3 | 19.3 / 38.2 / 38.2 | 8.24 / 22.09 | 10.05 / 13.80 | 9.81 | enemy 177 m; 6 live AFV; rm events 1, sparks 0, parcels 223, lights 9; lights offered 9; storm 0.58; station Loader |
| 252 s | 55.0 | 17.9 / 34.7 / 38.3 | 5.61 / 20.84 | 10.68 / 13.07 | 11.07 | enemy 185 m; 6 live AFV; 2 shells in flight; rm events 2, sparks 0, parcels 231, lights 9; lights offered 9; storm 0.58; station Loader |
| 26 s | 57.1 | 17.5 / 33.3 / 33.4 | 3.99 / 10.80 | 9.52 / 11.57 | — | enemy 449 m; 11 live AFV; rm events 0, sparks 0, parcels 253, lights 11; lights offered 11; storm 0.58; station Commander; beat 2 'Phase line Anton: the hamlet' |

GPU passes from 148 fenced frames (pass sum: median 10.01 ms, mean 9.82 ms; fenced frame wall median 20.26 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `raster-optic-exterior` | 2.293 | 22.9 % | 2.042 | 3.513 | 4.393 | 138/148 |
| 2 | `rm-gbuffer` | 1.719 | 17.2 % | 1.708 | 2.672 | 3.782 | 148/148 |
| 3 | `rm-point-face` | 1.218 | 12.2 % | 1.386 | 2.461 | 3.538 | 148/148 |
| 4 | `rm-shadow-far` | 0.914 | 9.1 % | 0.933 | 1.295 | 1.715 | 148/148 |
| 5 | `rm-sky` | 0.556 | 5.6 % | 0.594 | 0.796 | 3.219 | 148/148 |
| 6 | `rm-shade` | 0.498 | 5.0 % | 0.533 | 0.715 | 0.774 | 148/148 |
| 7 | `rm-shadow-near` | 0.488 | 4.9 % | 0.622 | 1.001 | 1.436 | 148/148 |
| 8 | `rm-sky-cover` | 0.476 | 4.8 % | 0.492 | 0.803 | 1.074 | 148/148 |
| 9 | `sky-visibility-dynamic` | 0.322 | 3.2 % | 0.326 | 0.547 | 0.759 | 138/148 |
| 10 | `raster-optic-current-weather` | 0.279 | 2.8 % | 0.264 | 0.552 | 0.697 | 138/148 |
| 11 | `exposure` | 0.158 | 1.6 % | 0.203 | 0.315 | 0.318 | 148/148 |
| 12 | `rm-volume` | 0.104 | 1.0 % | 0.129 | 0.217 | 0.243 | 148/148 |
| 13 | `rm-integrate` | 0.053 | 0.5 % | 0.070 | 0.100 | 0.494 | 148/148 |
| 14 | `rm-glass` | 0.043 | 0.4 % | 0.036 | 0.051 | 0.053 | 143/148 |
| 15 | `rm-reflect` | 0.042 | 0.4 % | 0.042 | 0.048 | 0.055 | 148/148 |
| 16 | `rm-cloud-shadow` | 0.034 | 0.3 % | 0.042 | 0.070 | 0.071 | 148/148 |
| 17 | `bloom-glare` | 0.011 | 0.1 % | 0.015 | 0.023 | 0.024 | 148/148 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.009 | 0.015 | 0.017 | 148/148 |
| 19 | `bloom-composite` | 0.007 | 0.1 % | 0.008 | 0.013 | 0.013 | 148/148 |
| 20 | `raster-optic-sky` | 0.007 | 0.1 % | 0.174 | 0.630 | 1.102 | 138/148 |
| 21 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.008 | 0.012 | 0.020 | 148/148 |
| 22 | `crush` | 0.005 | 0.1 % | 0.007 | 0.011 | 0.012 | 148/148 |
| 23 | `rm-points` | 0.004 | 0.0 % | 0.010 | 0.012 | 0.129 | 148/148 |
| 24 | `bloom-bright` | 0.004 | 0.0 % | 0.006 | 0.008 | 0.009 | 148/148 |
| 25 | `bloom-blur-h` | 0.003 | 0.0 % | 0.004 | 0.006 | 0.007 | 148/148 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (297 s) | 0.01 | 0.16 | 0.03 | 0.02 | 0.04 | 1.27 | 0.73 | 0.01 | 0.01 | 0.01 | 1.91 | 4.09 |
| enemy > 420 m (35 s) | 0.01 | 0.16 | 0.02 | 0.02 | 0.03 | 0.40 | 0.72 | 0.00 | 0.01 | 0.01 | 1.49 | 2.80 |
| enemy 150-420 m (230 s) | 0.01 | 0.16 | 0.03 | 0.01 | 0.04 | 1.28 | 0.71 | 0.01 | 0.01 | 0.01 | 1.91 | 4.10 |
| enemy < 150 m (32 s) | 0.01 | 0.17 | 0.04 | 0.02 | 0.02 | 2.08 | 0.85 | 0.00 | 0.01 | 0.01 | 2.31 | 5.44 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 10 | 12 | 13 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 240 | 266 | 1545 |
| cell_items | 0 | 36657 | 111214 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 1 | 6 |
| tracer_records | 0 | 0 | 4 |
| events | 1 | 3 | 3 |
| prepare_ms | 0.45 | 0.63 | 0.91 |
| encode_ms | 2.11 | 3.71 | 5.14 |
| point lights offered | 10 | 11 | 12 |
| live instances | 34029 | 34249 | 35485 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.39756202697754 |
| first_main_gun_shot_s | 34.44713592529297 |
| first_hit_taken_s | 37.47502517700195 |
| main_gun_shots | 9 |
| hits_dealt | 5 |
| kills | 5 |
| hits_taken | 2 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2453.355712890625 |
| modules_out | [] |
| reload_s | 8 reloads; median 5.1 s, max 10.1 s |
| stalls | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.1 s: [2, 'Phase line Anton: the hamlet']
- 123.4 s: [3, 'The gun line at the fork']
- 173.9 s: None

