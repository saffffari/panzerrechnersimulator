### artifacts/runtime/session-20261028-181756.jsonl

Session log `artifacts/runtime/session-20261028-181756.jsonl`: 415 one-second lines, 414 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 18273 frames over 414 s.

| metric | value |
|---|---|
| mean frame time (exact) | 23.00 ms (43.5 fps) |
| median second fps / 5th-percentile second fps | 45.9 / 25.7 |
| p50 of per-second p50 | 20.74 ms |
| p95 of per-second p95 | 62.62 ms |
| p99 of per-second p99 | 480.87 ms |
| worst single frame | 3091.1 ms |
| seconds with p99 > 33.3 ms | 191 |
| seconds with p50 > 16.7 ms | 376 |
| CPU simulation mean / p99-of-p99 | 7.62 / 183.00 ms |
| CPU render submission mean / p99-of-p99 | 13.15 / 48.22 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 2.22 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 54.9 | 16.69 | 32.01 | 54.84 | 654.5 | 5.00 | 11.94 | 0.58 | 11 |
| 1 | 45.5 | 21.19 | 37.62 | 68.19 | 152.8 | 7.65 | 14.05 | 0.58 | 25 |
| 2 | 46.9 | 20.36 | 33.97 | 66.61 | 170.3 | 7.02 | 13.64 | 0.58 | 60 |
| 3 | 45.9 | 20.82 | 33.87 | 194.15 | 212.3 | 7.53 | 14.01 | 0.58 | 59 |
| 4 | 29.9 | 27.52 | 326.63 | 627.16 | 3091.1 | 12.27 | 14.23 | 0.52 | 53 |
| 5 | 32.4 | 28.79 | 46.15 | 221.30 | 224.7 | 7.33 | 12.44 | 0.13 | 57 |
| 6 | 47.1 | 18.59 | 60.92 | 166.17 | 346.4 | 7.19 | 12.05 | 0.10 | 60 |
| 7 | 54.0 | 18.20 | 20.69 | 26.94 | 31.8 | 6.44 | 11.52 | 0.10 | 6 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 252 s | 29.3 | 134.6 / 3091.1 / 3091.1 | 14.08 / 61.92 | 14.29 / 47.87 | — | enemy 169 m; 6 live AFV; rm events 2, sparks 0, parcels 704, lights 12; lights offered 12; storm 0.58; station Loader; beat 3 'The gun line at the fork' |
| 1 s | 18.0 | 56.9 / 654.5 / 654.5 | 40.14 / 617.12 | 13.28 / 41.35 | — | enemy 652 m; 11 live AFV; rm events 0, sparks 0, parcels 14, lights 10; lights offered 10; storm 0.58; station Commander; beat 1 'Read the ground' |
| 242 s | 5.4 | 158.1 / 627.2 / 627.2 | 73.41 / 348.93 | 31.51 / 84.46 | 90.95 | enemy 23 m; 7 live AFV; 1 shells in flight; rm events 2, sparks 0, parcels 675, lights 12; lights offered 12; storm 0.58; station Gunner; optic; beat 3 'The gun line at the fork' |
| 261 s | 21.0 | 67.9 / 519.3 / 519.3 | 21.50 / 71.24 | 25.37 / 72.58 | — | enemy 168 m; 6 live AFV; 2 shells in flight; rm events 2, sparks 1, parcels 713, lights 11; lights offered 11; storm 0.58; station Loader; beat 3 'The gun line at the fork' |
| 259 s | 3.4 | 159.0 / 480.9 / 480.9 | 24.25 / 29.84 | 36.77 / 48.22 | 131.97 | enemy 168 m; 6 live AFV; rm events 1, sparks 0, parcels 704, lights 12; lights offered 12; storm 0.58; station Gunner; optic; beat 3 'The gun line at the fork' |

GPU passes from 206 fenced frames (pass sum: median 12.04 ms, mean 17.72 ms; fenced frame wall median 25.46 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `raster-optic-exterior` | 2.367 | 19.7 % | 3.276 | 15.929 | 40.039 | 196/206 |
| 2 | `sky-visibility-static` | 2.290 | 19.0 % | 4.733 | 16.199 | 119.022 | 179/206 |
| 3 | `rm-gbuffer` | 1.599 | 13.3 % | 2.136 | 6.603 | 17.539 | 206/206 |
| 4 | `rm-point-face` | 1.235 | 10.3 % | 1.942 | 4.222 | 26.306 | 206/206 |
| 5 | `rm-shadow-far` | 0.818 | 6.8 % | 1.043 | 1.403 | 9.075 | 206/206 |
| 6 | `rm-sky` | 0.572 | 4.8 % | 0.599 | 0.771 | 3.267 | 206/206 |
| 7 | `rm-shade` | 0.515 | 4.3 % | 0.560 | 0.777 | 0.860 | 206/206 |
| 8 | `rm-sky-cover` | 0.475 | 3.9 % | 0.810 | 1.138 | 16.189 | 206/206 |
| 9 | `rm-shadow-near` | 0.450 | 3.7 % | 0.715 | 0.982 | 12.827 | 206/206 |
| 10 | `sky-visibility-dynamic` | 0.310 | 2.6 % | 0.442 | 0.488 | 14.014 | 196/206 |
| 11 | `raster-optic-current-weather` | 0.306 | 2.5 % | 0.287 | 0.565 | 0.780 | 196/206 |
| 12 | `rm-volume` | 0.198 | 1.6 % | 0.264 | 0.509 | 7.972 | 206/206 |
| 13 | `exposure` | 0.158 | 1.3 % | 0.262 | 0.316 | 6.359 | 206/206 |
| 14 | `rm-integrate` | 0.053 | 0.4 % | 0.093 | 0.103 | 5.190 | 206/206 |
| 15 | `rm-reflect` | 0.044 | 0.4 % | 0.043 | 0.053 | 0.060 | 206/206 |
| 16 | `rm-glass` | 0.044 | 0.4 % | 0.039 | 0.052 | 0.056 | 203/206 |
| 17 | `rm-cloud-shadow` | 0.034 | 0.3 % | 0.042 | 0.070 | 0.074 | 206/206 |
| 18 | `bloom-glare` | 0.011 | 0.1 % | 0.015 | 0.023 | 0.024 | 206/206 |
| 19 | `bloom-glare-down` | 0.008 | 0.1 % | 0.009 | 0.015 | 0.018 | 206/206 |
| 20 | `bloom-composite` | 0.007 | 0.1 % | 0.009 | 0.013 | 0.013 | 206/206 |
| 21 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.008 | 0.012 | 0.012 | 206/206 |
| 22 | `raster-optic-sky` | 0.006 | 0.0 % | 0.147 | 0.497 | 2.036 | 196/206 |
| 23 | `crush` | 0.005 | 0.0 % | 0.007 | 0.011 | 0.012 | 206/206 |
| 24 | `bloom-bright` | 0.005 | 0.0 % | 0.006 | 0.009 | 0.010 | 206/206 |
| 25 | `rm-points` | 0.004 | 0.0 % | 0.009 | 0.011 | 0.215 | 206/206 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (414 s) | 0.01 | 0.17 | 0.04 | 0.02 | 0.03 | 4.14 | 0.84 | 0.23 | 0.01 | 0.01 | 2.20 | 7.62 |
| enemy > 420 m (83 s) | 0.01 | 0.15 | 0.03 | 0.02 | 0.03 | 3.11 | 0.83 | 0.00 | 0.01 | 0.01 | 1.98 | 6.10 |
| enemy 150-420 m (287 s) | 0.01 | 0.17 | 0.05 | 0.02 | 0.03 | 4.20 | 0.84 | 0.14 | 0.01 | 0.01 | 2.30 | 7.68 |
| enemy < 150 m (44 s) | 0.01 | 0.21 | 0.05 | 0.02 | 0.05 | 5.74 | 0.88 | 1.29 | 0.01 | 0.01 | 1.96 | 10.10 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 12 | 12 | 13 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 675 | 722 | 1990 |
| cell_items | 876 | 27344 | 116801 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 1 | 3 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 3 | 3 |
| prepare_ms | 0.61 | 0.89 | 1.18 |
| encode_ms | 3.31 | 4.02 | 18.10 |
| point lights offered | 11 | 12 | 13 |
| live instances | 34632 | 34774 | 36026 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.065532684326172 |
| first_main_gun_shot_s | 35.13425827026367 |
| first_hit_taken_s | 117.95393371582031 |
| main_gun_shots | 8 |
| hits_dealt | 5 |
| kills | 5 |
| hits_taken | 7 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2445.713623046875 |
| modules_out | ['LeftTrack'] |
| reload_s | 8 reloads; median 5.6 s, max 10.0 s |
| stalls | [{'from_s': 40.170265197753906, 'to_s': 86.6149673461914, 'at': [-746.0442504882812, -16.89725112915039, 538.5489501953125]}] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.8 s: [2, 'Phase line Anton: the hamlet']
- 113.9 s: [3, 'The gun line at the fork']
- 261.5 s: None

