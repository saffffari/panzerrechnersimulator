### artifacts/runtime/session-20261028-210554.jsonl

Session log `artifacts/runtime/session-20261028-210554.jsonl`: 298 one-second lines, 297 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 17380 frames over 297 s.

| metric | value |
|---|---|
| mean frame time (exact) | 17.12 ms (58.4 fps) |
| median second fps / 5th-percentile second fps | 59.1 / 51.4 |
| p50 of per-second p50 | 16.67 ms |
| p95 of per-second p95 | 23.23 ms |
| p99 of per-second p99 | 45.94 ms |
| worst single frame | 78.9 ms |
| seconds with p99 > 33.3 ms | 9 |
| seconds with p50 > 16.7 ms | 106 |
| CPU simulation mean / p99-of-p99 | 4.55 / 23.73 ms |
| CPU render submission mean / p99-of-p99 | 9.43 / 16.74 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 3.14 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.7 | 16.67 | 20.69 | 45.45 | 78.9 | 2.95 | 8.62 | 0.58 | 7 |
| 1 | 60.0 | 16.67 | 18.68 | 25.54 | 29.7 | 3.50 | 9.27 | 0.58 | 4 |
| 2 | 59.8 | 16.66 | 21.65 | 33.16 | 48.3 | 3.50 | 9.23 | 0.58 | 47 |
| 3 | 56.9 | 16.72 | 31.10 | 58.54 | 68.2 | 5.64 | 9.84 | 0.58 | 28 |
| 4 | 55.5 | 17.55 | 23.23 | 41.72 | 46.7 | 7.21 | 10.22 | 0.49 | 0 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 204 s | 39.8 | 25.1 / 68.2 / 68.2 | 4.46 / 11.57 | 8.54 / 10.89 | — | enemy 367 m; 6 live AFV; rm events 2, sparks 0, parcels 248, lights 9; lights offered 9; storm 0.58; station Loader |
| 211 s | 42.2 | 23.3 / 58.5 / 58.5 | 13.67 / 49.26 | 8.89 / 10.73 | 8.27 | enemy 1259 m; 5 live AFV; rm events 4, sparks 5, parcels 244, lights 12; lights offered 10; storm 0.58; station Loader |
| 298 s | 48.0 | 20.4 / 46.7 / 46.7 | 7.63 / 26.85 | 11.93 / 14.16 | 7.50 | enemy 1184 m; 5 live AFV; 2 shells in flight; rm events 1, sparks 0, parcels 748, lights 10; lights offered 10; storm 0.25; station Loader |
| 1 s | 54.7 | 18.7 / 45.9 / 78.9 | 4.25 / 11.84 | 9.35 / 12.93 | — | enemy 651 m; 11 live AFV; rm events 0, sparks 0, parcels 22, lights 10; lights offered 10; storm 0.58; station Commander; beat 1 'Read the ground' |
| 38 s | 55.1 | 17.9 / 45.5 / 52.6 | 5.78 / 15.07 | 7.69 / 10.18 | 7.29 | enemy 430 m; 10 live AFV; rm events 1, sparks 1, parcels 248, lights 12; lights offered 11; storm 0.58; station Loader; beat 2 'Phase line Anton: the hamlet' |

GPU passes from 149 fenced frames (pass sum: median 9.52 ms, mean 9.52 ms; fenced frame wall median 18.85 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-gbuffer` | 1.648 | 17.3 % | 1.619 | 2.676 | 2.898 | 149/149 |
| 2 | `raster-optic-exterior` | 1.640 | 17.2 % | 1.699 | 3.423 | 4.189 | 141/149 |
| 3 | `rm-point-face` | 1.198 | 12.6 % | 1.443 | 2.570 | 3.167 | 149/149 |
| 4 | `rm-shadow-far` | 0.835 | 8.8 % | 0.872 | 1.006 | 1.529 | 149/149 |
| 5 | `rm-shade` | 0.558 | 5.9 % | 0.571 | 0.728 | 0.772 | 149/149 |
| 6 | `rm-sky` | 0.541 | 5.7 % | 0.589 | 0.811 | 3.117 | 149/149 |
| 7 | `rm-shadow-near` | 0.476 | 5.0 % | 0.658 | 0.942 | 1.299 | 149/149 |
| 8 | `rm-sky-cover` | 0.468 | 4.9 % | 0.465 | 0.522 | 1.117 | 149/149 |
| 9 | `sky-visibility-dynamic` | 0.317 | 3.3 % | 0.318 | 0.527 | 0.745 | 141/149 |
| 10 | `raster-optic-current-weather` | 0.193 | 2.0 % | 0.215 | 0.534 | 0.968 | 141/149 |
| 11 | `raster-optic-sky` | 0.169 | 1.8 % | 0.243 | 0.711 | 2.360 | 141/149 |
| 12 | `exposure` | 0.159 | 1.7 % | 0.235 | 0.316 | 0.673 | 149/149 |
| 13 | `rm-volume` | 0.122 | 1.3 % | 0.173 | 0.481 | 0.665 | 149/149 |
| 14 | `rm-integrate` | 0.059 | 0.6 % | 0.075 | 0.108 | 0.113 | 149/149 |
| 15 | `rm-glass` | 0.046 | 0.5 % | 0.038 | 0.053 | 0.056 | 136/149 |
| 16 | `rm-reflect` | 0.043 | 0.5 % | 0.042 | 0.054 | 0.060 | 149/149 |
| 17 | `rm-cloud-shadow` | 0.036 | 0.4 % | 0.046 | 0.070 | 0.074 | 149/149 |
| 18 | `bloom-glare` | 0.011 | 0.1 % | 0.016 | 0.024 | 0.024 | 149/149 |
| 19 | `bloom-glare-down` | 0.008 | 0.1 % | 0.010 | 0.015 | 0.017 | 149/149 |
| 20 | `bloom-composite` | 0.007 | 0.1 % | 0.009 | 0.013 | 0.013 | 149/149 |
| 21 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.009 | 0.012 | 0.017 | 149/149 |
| 22 | `crush` | 0.006 | 0.1 % | 0.008 | 0.011 | 0.012 | 149/149 |
| 23 | `bloom-bright` | 0.005 | 0.1 % | 0.008 | 0.009 | 0.225 | 149/149 |
| 24 | `rm-points` | 0.004 | 0.0 % | 0.036 | 0.405 | 0.724 | 149/149 |
| 25 | `bloom-blur-h` | 0.003 | 0.0 % | 0.004 | 0.007 | 0.007 | 149/149 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (297 s) | 0.01 | 0.14 | 0.03 | 0.01 | 0.02 | 1.62 | 0.72 | 0.01 | 0.01 | 0.09 | 1.96 | 4.55 |
| enemy > 420 m (211 s) | 0.01 | 0.13 | 0.03 | 0.01 | 0.02 | 2.10 | 0.71 | 0.00 | 0.01 | 0.12 | 1.91 | 4.99 |
| enemy 150-420 m (86 s) | 0.01 | 0.16 | 0.04 | 0.02 | 0.05 | 0.47 | 0.72 | 0.01 | 0.01 | 0.01 | 2.08 | 3.47 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 11 | 12 | 15 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 245 | 742 | 792 |
| cell_items | 8 | 31000 | 195248 |
| embers | 0 | 0 | 0 |
| spark_records | 0 | 1 | 5 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 3 | 4 |
| prepare_ms | 0.46 | 0.67 | 1.02 |
| encode_ms | 1.58 | 3.11 | 7.42 |
| point lights offered | 10 | 11 | 12 |
| live instances | 34051 | 34895 | 36032 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.406980514526367 |
| first_main_gun_shot_s | 35.47288131713867 |
| first_hit_taken_s | 157.68161010742188 |
| main_gun_shots | 9 |
| hits_dealt | 6 |
| kills | 6 |
| hits_taken | 1 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2443.134521484375 |
| modules_out | [] |
| reload_s | 7 reloads; median 5.1 s, max 16.2 s |
| stalls | [{'from_s': 36.48674392700195, 'to_s': 85.98003387451172, 'at': [-745.9945678710938, -16.896560668945312, 538.646240234375]}, {'from_s': 210.17007446289062, 'to_s': 255.7200927734375, 'at': [-747.0701293945312, -21.4161319732666, 287.67950439453125]}] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.1 s: [2, 'Phase line Anton: the hamlet']
- 114.3 s: [3, 'The gun line at the fork']
- 169.8 s: None

