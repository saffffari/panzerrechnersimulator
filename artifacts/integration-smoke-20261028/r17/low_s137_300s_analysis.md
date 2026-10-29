### artifacts/runtime/session-20261028-215749.jsonl

Session log `artifacts/runtime/session-20261028-215749.jsonl`: 288 one-second lines, 287 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 16355 frames over 287 s.

| metric | value |
|---|---|
| mean frame time (exact) | 17.56 ms (56.9 fps) |
| median second fps / 5th-percentile second fps | 59.0 / 43.1 |
| p50 of per-second p50 | 16.66 ms |
| p95 of per-second p95 | 31.41 ms |
| p99 of per-second p99 | 79.41 ms |
| worst single frame | 143.0 ms |
| seconds with p99 > 33.3 ms | 37 |
| seconds with p50 > 16.7 ms | 93 |
| CPU simulation mean / p99-of-p99 | 4.12 / 25.37 ms |
| CPU render submission mean / p99-of-p99 | 10.62 / 25.51 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 2.82 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 57.7 | 16.67 | 27.42 | 45.94 | 88.7 | 3.48 | 10.18 | 0.58 | 26 |
| 1 | 60.0 | 16.66 | 18.63 | 22.75 | 34.2 | 3.22 | 9.54 | 0.58 | 35 |
| 2 | 56.1 | 16.67 | 32.82 | 50.38 | 60.2 | 3.99 | 11.40 | 0.58 | 60 |
| 3 | 54.6 | 16.67 | 36.58 | 79.41 | 143.0 | 4.85 | 11.22 | 0.58 | 59 |
| 4 | 56.2 | 16.66 | 31.18 | 80.18 | 80.2 | 5.28 | 10.79 | 0.54 | 49 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 218 s | 45.8 | 24.4 / 143.0 / 143.0 | 5.18 / 25.57 | 9.63 / 11.62 | — | enemy 260 m; 8 live AFV; rm events 2, sparks 2, parcels 235, lights 9; lights offered 9; storm 0.58; station Commander; beat 3 'The gun line at the fork' |
| 44 s | 44.9 | 21.6 / 88.7 / 88.7 | 3.66 / 10.20 | 13.69 / 20.64 | 24.36 | enemy 418 m; 10 live AFV; rm events 3, sparks 0, parcels 254, lights 11; lights offered 11; storm 0.58; station Gunner; optic; beat 2 'Phase line Anton: the hamlet' |
| 283 s | 45.3 | 21.8 / 80.2 / 80.2 | 8.99 / 58.72 | 11.79 / 17.31 | 7.98 | enemy 138 m; 7 live AFV; rm events 1, sparks 0, parcels 221, lights 10; lights offered 9; storm 0.45; station Loader |
| 217 s | 25.0 | 34.9 / 79.4 / 79.4 | 9.61 / 23.08 | 10.34 / 12.05 | 36.30 | enemy 261 m; 8 live AFV; rm events 2, sparks 3, parcels 243, lights 10; lights offered 9; storm 0.58; station Commander; beat 3 'The gun line at the fork' |
| 258 s | 40.6 | 24.6 / 61.4 / 61.4 | 7.29 / 15.24 | 13.72 / 22.94 | — | enemy 94 m; 8 live AFV; rm events 2, sparks 2, parcels 229, lights 10; lights offered 9; storm 0.58; station Loader |

GPU passes from 143 fenced frames (pass sum: median 9.98 ms, mean 10.31 ms; fenced frame wall median 20.56 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `raster-optic-exterior` | 2.172 | 21.8 % | 1.934 | 3.659 | 4.214 | 136/143 |
| 2 | `rm-gbuffer` | 1.595 | 16.0 % | 1.673 | 2.800 | 3.231 | 143/143 |
| 3 | `rm-point-face` | 1.270 | 12.7 % | 1.524 | 2.489 | 3.425 | 143/143 |
| 4 | `rm-shadow-far` | 0.899 | 9.0 % | 0.913 | 1.128 | 1.898 | 143/143 |
| 5 | `rm-sky` | 0.553 | 5.5 % | 0.591 | 0.768 | 3.167 | 143/143 |
| 6 | `rm-shade` | 0.519 | 5.2 % | 0.561 | 0.744 | 0.783 | 143/143 |
| 7 | `rm-shadow-near` | 0.485 | 4.9 % | 0.660 | 0.942 | 1.132 | 143/143 |
| 8 | `rm-sky-cover` | 0.475 | 4.8 % | 0.465 | 0.517 | 1.322 | 143/143 |
| 9 | `sky-visibility-dynamic` | 0.316 | 3.2 % | 0.322 | 0.592 | 0.639 | 136/143 |
| 10 | `raster-optic-current-weather` | 0.267 | 2.7 % | 0.261 | 0.551 | 0.585 | 136/143 |
| 11 | `raster-optic-sky` | 0.170 | 1.7 % | 0.214 | 0.508 | 1.704 | 136/143 |
| 12 | `exposure` | 0.159 | 1.6 % | 0.232 | 0.316 | 0.597 | 143/143 |
| 13 | `rm-volume` | 0.121 | 1.2 % | 0.157 | 0.252 | 0.538 | 143/143 |
| 14 | `rm-integrate` | 0.059 | 0.6 % | 0.075 | 0.105 | 0.128 | 143/143 |
| 15 | `rm-glass` | 0.046 | 0.5 % | 0.042 | 0.053 | 0.056 | 142/143 |
| 16 | `rm-reflect` | 0.044 | 0.4 % | 0.044 | 0.054 | 0.055 | 143/143 |
| 17 | `rm-cloud-shadow` | 0.036 | 0.4 % | 0.047 | 0.071 | 0.075 | 143/143 |
| 18 | `bloom-glare` | 0.011 | 0.1 % | 0.016 | 0.023 | 0.024 | 143/143 |
| 19 | `rm-points` | 0.009 | 0.1 % | 0.019 | 0.105 | 0.284 | 143/143 |
| 20 | `bloom-glare-down` | 0.008 | 0.1 % | 0.010 | 0.015 | 0.016 | 143/143 |
| 21 | `bloom-composite` | 0.007 | 0.1 % | 0.009 | 0.013 | 0.013 | 143/143 |
| 22 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.009 | 0.012 | 0.013 | 143/143 |
| 23 | `crush` | 0.006 | 0.1 % | 0.008 | 0.012 | 0.012 | 143/143 |
| 24 | `bloom-bright` | 0.005 | 0.1 % | 0.006 | 0.009 | 0.010 | 143/143 |
| 25 | `bloom-blur-h` | 0.003 | 0.0 % | 0.004 | 0.006 | 0.007 | 143/143 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (287 s) | 0.01 | 0.20 | 0.04 | 0.02 | 0.05 | 1.16 | 0.80 | 0.02 | 0.01 | 0.01 | 1.91 | 4.12 |
| enemy > 420 m (58 s) | 0.01 | 0.17 | 0.02 | 0.02 | 0.04 | 0.48 | 0.74 | 0.00 | 0.01 | 0.01 | 1.78 | 3.19 |
| enemy 150-420 m (183 s) | 0.01 | 0.20 | 0.04 | 0.02 | 0.06 | 1.21 | 0.81 | 0.03 | 0.01 | 0.01 | 1.83 | 4.11 |
| enemy < 150 m (46 s) | 0.01 | 0.21 | 0.04 | 0.04 | 0.06 | 1.86 | 0.82 | 0.01 | 0.01 | 0.01 | 2.42 | 5.35 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 10 | 11 | 14 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 242 | 266 | 1525 |
| cell_items | 0 | 39999 | 107713 |
| embers | 0 | 0 | 1397 |
| spark_records | 0 | 1 | 4 |
| tracer_records | 0 | 0 | 6 |
| events | 0 | 3 | 5 |
| prepare_ms | 0.47 | 0.76 | 1.06 |
| encode_ms | 1.83 | 3.61 | 5.49 |
| point lights offered | 10 | 11 | 11 |
| live instances | 33915 | 34156 | 35399 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.396780014038086 |
| first_main_gun_shot_s | 35.462581634521484 |
| first_hit_taken_s | 132.44384765625 |
| main_gun_shots | 8 |
| hits_dealt | 5 |
| kills | 4 |
| hits_taken | 9 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2442.10693359375 |
| modules_out | ['HullMg'] |
| reload_s | 8 reloads; median 5.1 s, max 9.2 s |
| stalls | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.1 s: [2, 'Phase line Anton: the hamlet']
- 114.3 s: [3, 'The gun line at the fork']
- 228.4 s: None

