### artifacts/runtime/session-20261028-193348.jsonl

Session log `artifacts/runtime/session-20261028-193348.jsonl`: 298 one-second lines, 297 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 17678 frames over 297 s.

| metric | value |
|---|---|
| mean frame time (exact) | 16.85 ms (59.4 fps) |
| median second fps / 5th-percentile second fps | 59.1 / 58.4 |
| p50 of per-second p50 | 16.67 ms |
| p95 of per-second p95 | 21.20 ms |
| p99 of per-second p99 | 96.02 ms |
| worst single frame | 234.3 ms |
| seconds with p99 > 33.3 ms | 12 |
| seconds with p50 > 16.7 ms | 79 |
| CPU simulation mean / p99-of-p99 | 3.66 / 22.88 ms |
| CPU render submission mean / p99-of-p99 | 9.02 / 17.50 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 4.17 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.9 | 16.67 | 19.12 | 32.40 | 77.8 | 2.78 | 8.40 | 0.58 | 7 |
| 1 | 60.0 | 16.67 | 17.86 | 22.93 | 30.9 | 3.01 | 8.81 | 0.58 | 4 |
| 2 | 59.6 | 16.67 | 23.39 | 35.49 | 48.6 | 3.74 | 9.24 | 0.58 | 45 |
| 3 | 58.9 | 16.66 | 18.43 | 96.02 | 114.5 | 4.25 | 10.00 | 0.58 | 57 |
| 4 | 58.3 | 16.67 | 40.68 | 173.17 | 234.3 | 4.55 | 8.62 | 0.49 | 59 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 268 s | 14.7 | 63.2 / 234.3 / 234.3 | 11.21 / 24.87 | 9.03 / 11.24 | 64.28 | enemy 262 m; 6 live AFV; rm events 1, sparks 0, parcels 213, lights 9; lights offered 9; storm 0.58; station Loader |
| 276 s | 41.2 | 22.7 / 173.2 / 173.2 | 5.83 / 27.54 | 10.26 / 18.45 | 60.75 | enemy 262 m; 6 live AFV; 1 shells in flight; 1 live blasts; rm events 3, sparks 1, parcels 243, lights 11; lights offered 10; storm 0.52; station Loader |
| 230 s | 28.2 | 34.4 / 114.5 / 114.5 | 8.18 / 22.88 | 14.63 / 17.50 | 14.07 | enemy 421 m; 6 live AFV; rm events 2, sparks 5, parcels 247, lights 10; lights offered 9; storm 0.58; station Commander |
| 231 s | 27.8 | 36.0 / 96.0 / 96.0 | 7.86 / 27.41 | 9.56 / 11.55 | — | enemy 420 m; 6 live AFV; rm events 2, sparks 3, parcels 248, lights 9; lights offered 9; storm 0.58; station Commander |
| 278 s | 47.7 | 20.7 / 46.6 / 46.6 | 5.54 / 14.15 | 7.76 / 10.13 | 7.01 | enemy 262 m; 6 live AFV; rm events 3, sparks 3, parcels 241, lights 9; lights offered 9; storm 0.50; station Loader |

GPU passes from 148 fenced frames (pass sum: median 9.43 ms, mean 10.17 ms; fenced frame wall median 18.52 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `raster-optic-exterior` | 2.048 | 21.7 % | 1.819 | 3.361 | 4.379 | 143/148 |
| 2 | `rm-gbuffer` | 1.636 | 17.4 % | 1.577 | 2.421 | 2.779 | 148/148 |
| 3 | `rm-point-face` | 1.207 | 12.8 % | 1.400 | 2.452 | 3.610 | 148/148 |
| 4 | `rm-shadow-far` | 0.827 | 8.8 % | 0.861 | 0.996 | 1.233 | 148/148 |
| 5 | `rm-sky` | 0.545 | 5.8 % | 0.586 | 0.796 | 3.140 | 148/148 |
| 6 | `rm-shade` | 0.521 | 5.5 % | 0.559 | 0.706 | 0.780 | 148/148 |
| 7 | `rm-shadow-near` | 0.469 | 5.0 % | 0.637 | 0.922 | 1.133 | 148/148 |
| 8 | `rm-sky-cover` | 0.459 | 4.9 % | 0.444 | 0.504 | 0.753 | 148/148 |
| 9 | `sky-visibility-dynamic` | 0.311 | 3.3 % | 0.313 | 0.394 | 0.658 | 143/148 |
| 10 | `raster-optic-current-weather` | 0.275 | 2.9 % | 0.236 | 0.536 | 0.729 | 143/148 |
| 11 | `raster-optic-sky` | 0.170 | 1.8 % | 0.221 | 0.530 | 1.793 | 143/148 |
| 12 | `exposure` | 0.159 | 1.7 % | 0.237 | 0.339 | 0.605 | 148/148 |
| 13 | `rm-volume` | 0.116 | 1.2 % | 0.155 | 0.238 | 0.971 | 148/148 |
| 14 | `rm-integrate` | 0.059 | 0.6 % | 0.076 | 0.108 | 0.115 | 148/148 |
| 15 | `rm-glass` | 0.045 | 0.5 % | 0.042 | 0.053 | 0.056 | 144/148 |
| 16 | `rm-reflect` | 0.044 | 0.5 % | 0.044 | 0.054 | 0.063 | 148/148 |
| 17 | `rm-cloud-shadow` | 0.035 | 0.4 % | 0.046 | 0.070 | 0.071 | 148/148 |
| 18 | `bloom-glare` | 0.012 | 0.1 % | 0.016 | 0.024 | 0.024 | 148/148 |
| 19 | `bloom-glare-down` | 0.009 | 0.1 % | 0.012 | 0.016 | 0.202 | 148/148 |
| 20 | `rm-points` | 0.008 | 0.1 % | 0.015 | 0.011 | 0.836 | 148/148 |
| 21 | `bloom-composite` | 0.007 | 0.1 % | 0.011 | 0.013 | 0.200 | 148/148 |
| 22 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.009 | 0.012 | 0.013 | 148/148 |
| 23 | `crush` | 0.006 | 0.1 % | 0.008 | 0.011 | 0.012 | 148/148 |
| 24 | `bloom-bright` | 0.005 | 0.1 % | 0.006 | 0.009 | 0.011 | 148/148 |
| 25 | `bloom-blur-h` | 0.003 | 0.0 % | 0.005 | 0.006 | 0.007 | 148/148 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (297 s) | 0.01 | 0.14 | 0.03 | 0.02 | 0.03 | 0.79 | 0.68 | 0.01 | 0.01 | 0.01 | 2.02 | 3.66 |
| enemy > 420 m (125 s) | 0.01 | 0.14 | 0.03 | 0.02 | 0.02 | 0.44 | 0.69 | 0.01 | 0.01 | 0.01 | 1.77 | 3.08 |
| enemy 150-420 m (172 s) | 0.01 | 0.14 | 0.04 | 0.01 | 0.03 | 1.04 | 0.68 | 0.01 | 0.01 | 0.01 | 2.20 | 4.09 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 10 | 11 | 12 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 241 | 266 | 1485 |
| cell_items | 0 | 29138 | 110409 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 1 | 5 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 3 | 3 |
| prepare_ms | 0.45 | 0.57 | 0.91 |
| encode_ms | 1.74 | 3.28 | 5.46 |
| point lights offered | 10 | 11 | 11 |
| live instances | 33998 | 34184 | 35476 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.432920455932617 |
| first_main_gun_shot_s | 35.51670455932617 |
| first_hit_taken_s | 157.94906616210938 |
| main_gun_shots | 8 |
| hits_dealt | 5 |
| kills | 5 |
| hits_taken | 2 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2443.650634765625 |
| modules_out | [] |
| reload_s | 7 reloads; median 5.2 s, max 12.2 s |
| stalls | [{'from_s': 40.582828521728516, 'to_s': 86.0816879272461, 'at': [-745.9725952148438, -16.896268844604492, 538.663818359375]}] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.1 s: [2, 'Phase line Anton: the hamlet']
- 113.4 s: [3, 'The gun line at the fork']
- 229.7 s: None

