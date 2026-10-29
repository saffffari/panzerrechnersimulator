### artifacts/runtime/session-20261028-180039.jsonl

Session log `artifacts/runtime/session-20261028-180039.jsonl`: 594 one-second lines, 593 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 31168 frames over 593 s.

| metric | value |
|---|---|
| mean frame time (exact) | 19.09 ms (52.4 fps) |
| median second fps / 5th-percentile second fps | 58.7 / 12.9 |
| p50 of per-second p50 | 16.72 ms |
| p95 of per-second p95 | 89.06 ms |
| p99 of per-second p99 | 302.80 ms |
| worst single frame | 628.8 ms |
| seconds with p99 > 33.3 ms | 140 |
| seconds with p50 > 16.7 ms | 323 |
| CPU simulation mean / p99-of-p99 | 5.28 / 40.89 ms |
| CPU render submission mean / p99-of-p99 | 13.44 / 43.39 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 0.37 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.0 | 16.67 | 21.44 | 40.78 | 628.8 | 3.21 | 10.23 | 0.58 | 6 |
| 1 | 59.8 | 16.67 | 19.72 | 42.18 | 61.7 | 2.74 | 10.60 | 0.58 | 5 |
| 2 | 58.8 | 16.68 | 22.42 | 145.38 | 362.2 | 2.89 | 11.13 | 0.58 | 45 |
| 3 | 58.3 | 16.67 | 25.11 | 87.80 | 292.7 | 3.53 | 10.24 | 0.58 | 60 |
| 4 | 59.1 | 16.60 | 23.18 | 38.89 | 297.7 | 4.23 | 10.55 | 0.49 | 59 |
| 5 | 56.0 | 16.39 | 86.30 | 293.47 | 297.0 | 5.41 | 12.42 | 0.11 | 3 |
| 6 | 39.2 | 17.48 | 283.82 | 301.15 | 307.5 | 8.20 | 20.14 | 0.10 | 0 |
| 7 | 43.6 | 19.16 | 91.23 | 302.80 | 311.5 | 7.69 | 16.85 | 0.10 | 0 |
| 8 | 42.8 | 19.05 | 89.97 | 304.22 | 305.0 | 7.94 | 17.66 | 0.10 | 0 |
| 9 | 47.0 | 19.39 | 87.94 | 293.76 | 296.1 | 7.04 | 14.80 | 0.10 | 0 |
| 10 | 47.9 | 20.27 | 28.04 | 34.81 | 34.8 | 7.11 | 13.23 | 0.10 | 0 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 1 s | 20.0 | 51.1 / 628.8 / 628.8 | 34.87 / 592.39 | 12.40 / 43.00 | — | enemy 652 m; 11 live AFV; rm events 0, sparks 0, parcels 14, lights 10; lights offered 10; storm 0.58; station Commander; beat 1 'Read the ground' |
| 178 s | 47.8 | 27.9 / 362.2 / 362.2 | 5.63 / 38.60 | 10.19 / 38.78 | — | enemy 231 m; 8 live AFV; rm events 2, sparks 2, parcels 248, lights 9; lights offered 9; storm 0.58; station Loader |
| 432 s | 26.6 | 46.9 / 311.5 / 311.5 | 10.47 / 44.26 | 19.58 / 43.31 | — | enemy 1102 m; 5 live AFV; rm events 0, sparks 0, parcels 211, lights 10; lights offered 10; storm 0.10; station Commander |
| 374 s | 19.7 | 45.2 / 307.5 / 307.5 | 9.68 / 45.45 | 22.66 / 41.51 | 85.55 | enemy 1100 m; 5 live AFV; rm events 0, sparks 0, parcels 210, lights 10; lights offered 10; storm 0.10; station Commander |
| 527 s | 13.2 | 93.7 / 305.0 / 305.0 | 15.92 / 41.48 | 37.93 / 42.92 | — | enemy 1104 m; 5 live AFV; rm events 0, sparks 0, parcels 215, lights 10; lights offered 10; storm 0.10; station Commander |

GPU passes from 292 fenced frames (pass sum: median 11.71 ms, mean 19.48 ms; fenced frame wall median 24.13 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `sky-visibility-static` | 2.279 | 19.5 % | 10.018 | 77.259 | 77.644 | 257/292 |
| 2 | `raster-optic-exterior` | 2.151 | 18.4 % | 2.058 | 3.324 | 4.951 | 283/292 |
| 3 | `rm-point-face` | 1.357 | 11.6 % | 1.369 | 2.375 | 2.862 | 292/292 |
| 4 | `rm-gbuffer` | 1.211 | 10.3 % | 1.328 | 1.965 | 2.601 | 292/292 |
| 5 | `rm-shadow-far` | 0.793 | 6.8 % | 0.842 | 1.140 | 1.299 | 292/292 |
| 6 | `rm-sky` | 0.558 | 4.8 % | 0.582 | 0.726 | 3.188 | 292/292 |
| 7 | `rm-sky-cover` | 0.510 | 4.4 % | 0.492 | 0.539 | 0.824 | 292/292 |
| 8 | `rm-shade` | 0.493 | 4.2 % | 0.520 | 0.702 | 0.758 | 292/292 |
| 9 | `rm-shadow-near` | 0.469 | 4.0 % | 0.541 | 0.919 | 1.147 | 292/292 |
| 10 | `sky-visibility-dynamic` | 0.393 | 3.4 % | 0.396 | 0.609 | 0.935 | 283/292 |
| 11 | `raster-optic-current-weather` | 0.330 | 2.8 % | 0.399 | 0.877 | 1.461 | 283/292 |
| 12 | `raster-optic-sky` | 0.168 | 1.4 % | 0.255 | 1.033 | 2.581 | 283/292 |
| 13 | `exposure` | 0.157 | 1.3 % | 0.185 | 0.315 | 0.500 | 292/292 |
| 14 | `rm-volume` | 0.097 | 0.8 % | 0.119 | 0.214 | 0.497 | 292/292 |
| 15 | `rm-integrate` | 0.052 | 0.4 % | 0.061 | 0.105 | 0.112 | 292/292 |
| 16 | `rm-reflect` | 0.044 | 0.4 % | 0.044 | 0.053 | 0.065 | 292/292 |
| 17 | `rm-glass` | 0.043 | 0.4 % | 0.038 | 0.051 | 0.055 | 279/292 |
| 18 | `rm-cloud-shadow` | 0.033 | 0.3 % | 0.036 | 0.069 | 0.075 | 292/292 |
| 19 | `bloom-glare` | 0.011 | 0.1 % | 0.013 | 0.023 | 0.024 | 292/292 |
| 20 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.014 | 0.017 | 292/292 |
| 21 | `bloom-composite` | 0.007 | 0.1 % | 0.008 | 0.013 | 0.013 | 292/292 |
| 22 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.007 | 0.012 | 0.013 | 292/292 |
| 23 | `crush` | 0.005 | 0.0 % | 0.006 | 0.011 | 0.012 | 292/292 |
| 24 | `bloom-bright` | 0.004 | 0.0 % | 0.005 | 0.008 | 0.009 | 292/292 |
| 25 | `rm-points` | 0.003 | 0.0 % | 0.006 | 0.011 | 0.108 | 292/292 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (593 s) | 0.01 | 0.13 | 0.04 | 0.01 | 0.02 | 1.36 | 0.98 | 0.01 | 0.02 | 0.01 | 2.77 | 5.28 |
| enemy > 420 m (415 s) | 0.01 | 0.13 | 0.04 | 0.01 | 0.01 | 1.62 | 1.12 | 0.00 | 0.02 | 0.01 | 3.13 | 6.03 |
| enemy 150-420 m (161 s) | 0.01 | 0.15 | 0.04 | 0.02 | 0.03 | 0.68 | 0.66 | 0.02 | 0.01 | 0.01 | 1.90 | 3.44 |
| enemy < 150 m (17 s) | 0.01 | 0.15 | 0.03 | 0.01 | 0.04 | 1.39 | 0.68 | 0.02 | 0.01 | 0.01 | 2.29 | 4.56 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 10 | 11 | 12 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 221 | 263 | 1538 |
| cell_items | 0 | 29058 | 113559 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 0 | 4 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 3 | 4 |
| prepare_ms | 0.43 | 0.57 | 0.95 |
| encode_ms | 3.47 | 18.58 | 20.64 |
| point lights offered | 10 | 11 | 11 |
| live instances | 34199 | 35461 | 35760 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.109254837036133 |
| first_main_gun_shot_s | 35.18349838256836 |
| first_hit_taken_s | 156.49853515625 |
| main_gun_shots | 10 |
| hits_dealt | 6 |
| kills | 6 |
| hits_taken | 1 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2432.268310546875 |
| modules_out | [] |
| reload_s | 9 reloads; median 5.0 s, max 13.1 s |
| stalls | [{'from_s': 35.18349838256836, 'to_s': 85.7348861694336, 'at': [-746.140869140625, -16.89605140686035, 538.6093139648438]}, {'from_s': 453.5650939941406, 'to_s': 475.7960205078125, 'at': [-733.4032592773438, -22.638690948486328, 103.3452377319336]}] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.8 s: [2, 'Phase line Anton: the hamlet']
- 113.1 s: [3, 'The gun line at the fork']
- 176.5 s: None

