### artifacts/runtime/session-20261028-211449.jsonl

Session log `artifacts/runtime/session-20261028-211449.jsonl`: 298 one-second lines, 297 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 17788 frames over 297 s.

| metric | value |
|---|---|
| mean frame time (exact) | 16.74 ms (59.7 fps) |
| median second fps / 5th-percentile second fps | 59.3 / 57.8 |
| p50 of per-second p50 | 16.66 ms |
| p95 of per-second p95 | 21.37 ms |
| p99 of per-second p99 | 38.76 ms |
| worst single frame | 127.0 ms |
| seconds with p99 > 33.3 ms | 7 |
| seconds with p50 > 16.7 ms | 34 |
| CPU simulation mean / p99-of-p99 | 3.60 / 20.00 ms |
| CPU render submission mean / p99-of-p99 | 9.34 / 17.12 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 3.79 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.9 | 16.67 | 20.95 | 32.07 | 78.0 | 3.01 | 8.94 | 0.58 | 7 |
| 1 | 60.0 | 16.66 | 17.89 | 24.23 | 29.8 | 3.32 | 9.08 | 0.58 | 4 |
| 2 | 59.8 | 16.66 | 22.48 | 33.67 | 127.0 | 3.52 | 9.30 | 0.58 | 47 |
| 3 | 59.4 | 16.66 | 21.28 | 35.29 | 46.0 | 4.25 | 9.48 | 0.58 | 7 |
| 4 | 59.7 | 16.65 | 22.00 | 33.53 | 49.6 | 3.93 | 9.90 | 0.49 | 0 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 165 s | 46.9 | 20.7 / 127.0 / 127.0 | 3.52 / 9.75 | 12.81 / 19.05 | 42.70 | enemy 412 m; 7 live AFV; rm events 3, sparks 2, parcels 248, lights 10; lights offered 9; storm 0.58; station Loader |
| 1 s | 55.2 | 18.5 / 46.5 / 78.0 | 4.17 / 11.85 | 9.08 / 13.90 | — | enemy 651 m; 11 live AFV; rm events 0, sparks 0, parcels 22, lights 10; lights offered 10; storm 0.58; station Commander; beat 1 'Read the ground' |
| 231 s | 47.8 | 20.4 / 46.0 / 46.0 | 5.70 / 16.31 | 13.51 / 32.29 | 8.00 | enemy 1129 m; 5 live AFV; rm events 1, sparks 0, parcels 225, lights 9; lights offered 9; storm 0.58; station Loader |
| 294 s | 52.2 | 19.2 / 38.8 / 49.6 | 4.74 / 11.91 | 9.59 / 11.27 | — | enemy 1104 m; 5 live AFV; rm events 2, sparks 2, parcels 229, lights 9; lights offered 9; storm 0.30; station Loader |
| 229 s | 55.9 | 17.5 / 35.3 / 37.4 | 5.33 / 21.27 | 11.54 / 13.49 | 9.63 | enemy 1129 m; 5 live AFV; 2 shells in flight; rm events 1, sparks 0, parcels 226, lights 9; lights offered 9; storm 0.58; station Loader |

GPU passes from 148 fenced frames (pass sum: median 9.58 ms, mean 9.68 ms; fenced frame wall median 20.04 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `raster-optic-exterior` | 2.138 | 22.3 % | 1.986 | 3.466 | 4.327 | 139/148 |
| 2 | `rm-gbuffer` | 1.646 | 17.2 % | 1.546 | 2.448 | 3.361 | 148/148 |
| 3 | `rm-point-face` | 1.235 | 12.9 % | 1.334 | 2.593 | 3.183 | 148/148 |
| 4 | `rm-shadow-far` | 0.919 | 9.6 % | 0.932 | 1.298 | 1.696 | 148/148 |
| 5 | `rm-sky` | 0.558 | 5.8 % | 0.595 | 0.796 | 3.203 | 148/148 |
| 6 | `rm-shade` | 0.504 | 5.3 % | 0.524 | 0.684 | 0.757 | 148/148 |
| 7 | `rm-shadow-near` | 0.467 | 4.9 % | 0.571 | 0.953 | 1.176 | 148/148 |
| 8 | `rm-sky-cover` | 0.467 | 4.9 % | 0.481 | 0.738 | 1.159 | 148/148 |
| 9 | `sky-visibility-dynamic` | 0.317 | 3.3 % | 0.318 | 0.459 | 0.765 | 139/148 |
| 10 | `raster-optic-current-weather` | 0.287 | 3.0 % | 0.274 | 0.537 | 0.853 | 139/148 |
| 11 | `raster-optic-sky` | 0.168 | 1.8 % | 0.217 | 0.714 | 2.323 | 139/148 |
| 12 | `exposure` | 0.158 | 1.6 % | 0.202 | 0.315 | 0.369 | 148/148 |
| 13 | `rm-volume` | 0.101 | 1.1 % | 0.135 | 0.225 | 0.426 | 148/148 |
| 14 | `rm-integrate` | 0.056 | 0.6 % | 0.067 | 0.106 | 0.108 | 148/148 |
| 15 | `rm-reflect` | 0.043 | 0.4 % | 0.043 | 0.053 | 0.058 | 148/148 |
| 16 | `rm-glass` | 0.043 | 0.4 % | 0.038 | 0.052 | 0.055 | 145/148 |
| 17 | `rm-cloud-shadow` | 0.034 | 0.4 % | 0.041 | 0.070 | 0.073 | 148/148 |
| 18 | `bloom-glare` | 0.011 | 0.1 % | 0.014 | 0.023 | 0.024 | 148/148 |
| 19 | `bloom-composite` | 0.007 | 0.1 % | 0.008 | 0.013 | 0.013 | 148/148 |
| 20 | `bloom-glare-down` | 0.007 | 0.1 % | 0.009 | 0.015 | 0.021 | 148/148 |
| 21 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.008 | 0.012 | 0.013 | 148/148 |
| 22 | `crush` | 0.005 | 0.1 % | 0.007 | 0.011 | 0.014 | 148/148 |
| 23 | `bloom-bright` | 0.005 | 0.1 % | 0.006 | 0.009 | 0.009 | 148/148 |
| 24 | `rm-points` | 0.004 | 0.0 % | 0.007 | 0.011 | 0.108 | 148/148 |
| 25 | `bloom-blur-h` | 0.003 | 0.0 % | 0.004 | 0.006 | 0.008 | 148/148 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (297 s) | 0.01 | 0.13 | 0.03 | 0.02 | 0.02 | 0.76 | 0.74 | 0.00 | 0.01 | 0.01 | 1.94 | 3.60 |
| enemy > 420 m (232 s) | 0.01 | 0.13 | 0.03 | 0.01 | 0.02 | 0.78 | 0.74 | 0.00 | 0.01 | 0.01 | 1.97 | 3.65 |
| enemy 150-420 m (65 s) | 0.01 | 0.16 | 0.04 | 0.02 | 0.04 | 0.66 | 0.72 | 0.01 | 0.01 | 0.01 | 1.86 | 3.43 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 10 | 11 | 12 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 240 | 266 | 1578 |
| cell_items | 0 | 32256 | 157520 |
| embers | 0 | 0 | 1397 |
| spark_records | 0 | 1 | 4 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 3 | 3 |
| prepare_ms | 0.45 | 0.60 | 1.00 |
| encode_ms | 2.16 | 3.34 | 4.31 |
| point lights offered | 10 | 11 | 11 |
| live instances | 34083 | 34331 | 35520 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.485294342041016 |
| first_main_gun_shot_s | 35.51217269897461 |
| first_hit_taken_s | None |
| main_gun_shots | 9 |
| hits_dealt | 6 |
| kills | 6 |
| hits_taken | 0 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2452.13330078125 |
| modules_out | [] |
| reload_s | 9 reloads; median 5.1 s, max 7.1 s |
| stalls | [{'from_s': 40.55194091796875, 'to_s': 86.10122680664062, 'at': [-745.9063720703125, -16.895530700683594, 538.75634765625]}] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.2 s: [2, 'Phase line Anton: the hamlet']
- 113.4 s: [3, 'The gun line at the fork']
- 164.9 s: None

