### artifacts/runtime/session-20261028-184402.jsonl

Session log `artifacts/runtime/session-20261028-184402.jsonl`: 298 one-second lines, 297 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 17604 frames over 297 s.

| metric | value |
|---|---|
| mean frame time (exact) | 16.91 ms (59.1 fps) |
| median second fps / 5th-percentile second fps | 59.1 / 55.0 |
| p50 of per-second p50 | 16.65 ms |
| p95 of per-second p95 | 23.44 ms |
| p99 of per-second p99 | 144.84 ms |
| worst single frame | 337.2 ms |
| seconds with p99 > 33.3 ms | 22 |
| seconds with p50 > 16.7 ms | 76 |
| CPU simulation mean / p99-of-p99 | 3.54 / 25.84 ms |
| CPU render submission mean / p99-of-p99 | 10.51 / 39.74 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 2.86 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.3 | 16.67 | 22.63 | 47.49 | 74.7 | 2.69 | 10.13 | 0.58 | 25 |
| 1 | 59.7 | 16.65 | 21.51 | 39.34 | 102.7 | 2.80 | 10.65 | 0.58 | 59 |
| 2 | 58.4 | 16.68 | 22.95 | 169.62 | 337.2 | 3.65 | 10.52 | 0.58 | 59 |
| 3 | 59.0 | 16.55 | 24.06 | 95.06 | 146.0 | 3.81 | 11.00 | 0.58 | 60 |
| 4 | 59.4 | 16.60 | 24.84 | 37.95 | 59.3 | 4.78 | 10.24 | 0.49 | 44 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 162 s | 48.6 | 26.2 / 337.2 / 337.2 | 6.12 / 55.41 | 10.25 / 11.33 | — | enemy 147 m; 8 live AFV; rm events 2, sparks 1, parcels 237, lights 9; lights offered 9; storm 0.58; station Loader |
| 161 s | 4.8 | 142.1 / 169.6 / 169.6 | 18.94 / 26.72 | 34.56 / 40.07 | 60.17 | enemy 147 m; 8 live AFV; rm events 3, sparks 2, parcels 232, lights 9; lights offered 9; storm 0.58; station Loader |
| 208 s | 31.4 | 30.4 / 146.0 / 146.0 | 4.87 / 19.32 | 16.42 / 42.44 | 11.76 | enemy 28 m; 7 live AFV; 1 shells in flight; 1 live blasts; rm events 2, sparks 1, parcels 230, lights 11; lights offered 10; storm 0.58; station Loader |
| 160 s | 46.2 | 20.7 / 144.8 / 144.8 | 4.39 / 23.55 | 13.67 / 41.51 | 11.95 | enemy 147 m; 8 live AFV; 1 shells in flight; 1 live blasts; rm events 3, sparks 2, parcels 243, lights 12; lights offered 10; storm 0.58; station Loader |
| 209 s | 44.8 | 23.0 / 95.1 / 95.1 | 6.46 / 26.37 | 10.42 / 39.74 | — | enemy 28 m; 7 live AFV; rm events 2, sparks 2, parcels 221, lights 9; lights offered 9; storm 0.58; station Loader |

GPU passes from 148 fenced frames (pass sum: median 11.28 ms, mean 11.59 ms; fenced frame wall median 22.63 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `sky-visibility-static` | 2.250 | 20.0 % | 1.874 | 3.986 | 6.430 | 115/148 |
| 2 | `raster-optic-exterior` | 2.103 | 18.7 % | 2.023 | 3.595 | 4.219 | 141/148 |
| 3 | `rm-gbuffer` | 1.561 | 13.8 % | 1.479 | 2.089 | 2.379 | 148/148 |
| 4 | `rm-point-face` | 1.209 | 10.7 % | 1.297 | 2.584 | 2.906 | 148/148 |
| 5 | `rm-shadow-far` | 0.958 | 8.5 % | 0.940 | 1.171 | 1.285 | 148/148 |
| 6 | `rm-sky` | 0.550 | 4.9 % | 0.587 | 0.814 | 3.167 | 148/148 |
| 7 | `rm-shade` | 0.494 | 4.4 % | 0.523 | 0.693 | 0.783 | 148/148 |
| 8 | `rm-shadow-near` | 0.472 | 4.2 % | 0.596 | 0.976 | 1.184 | 148/148 |
| 9 | `rm-sky-cover` | 0.466 | 4.1 % | 0.457 | 0.530 | 0.760 | 148/148 |
| 10 | `sky-visibility-dynamic` | 0.307 | 2.7 % | 0.369 | 0.715 | 1.002 | 141/148 |
| 11 | `raster-optic-current-weather` | 0.294 | 2.6 % | 0.287 | 0.541 | 0.705 | 141/148 |
| 12 | `raster-optic-sky` | 0.169 | 1.5 % | 0.209 | 0.721 | 1.132 | 141/148 |
| 13 | `exposure` | 0.158 | 1.4 % | 0.208 | 0.317 | 0.500 | 148/148 |
| 14 | `rm-volume` | 0.100 | 0.9 % | 0.127 | 0.219 | 0.242 | 148/148 |
| 15 | `rm-integrate` | 0.052 | 0.5 % | 0.070 | 0.107 | 0.391 | 148/148 |
| 16 | `rm-glass` | 0.044 | 0.4 % | 0.037 | 0.051 | 0.061 | 142/148 |
| 17 | `rm-reflect` | 0.043 | 0.4 % | 0.044 | 0.058 | 0.064 | 148/148 |
| 18 | `rm-cloud-shadow` | 0.034 | 0.3 % | 0.041 | 0.070 | 0.072 | 148/148 |
| 19 | `bloom-glare` | 0.011 | 0.1 % | 0.015 | 0.023 | 0.024 | 148/148 |
| 20 | `bloom-composite` | 0.007 | 0.1 % | 0.008 | 0.013 | 0.013 | 148/148 |
| 21 | `bloom-glare-down` | 0.007 | 0.1 % | 0.009 | 0.016 | 0.017 | 148/148 |
| 22 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.008 | 0.012 | 0.013 | 148/148 |
| 23 | `crush` | 0.005 | 0.0 % | 0.007 | 0.011 | 0.012 | 148/148 |
| 24 | `rm-points` | 0.004 | 0.0 % | 0.007 | 0.011 | 0.106 | 148/148 |
| 25 | `bloom-bright` | 0.004 | 0.0 % | 0.006 | 0.009 | 0.011 | 148/148 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (297 s) | 0.01 | 0.16 | 0.03 | 0.02 | 0.04 | 0.82 | 0.68 | 0.01 | 0.01 | 0.01 | 1.85 | 3.54 |
| enemy > 420 m (50 s) | 0.01 | 0.14 | 0.03 | 0.02 | 0.02 | 0.64 | 0.67 | 0.00 | 0.01 | 0.01 | 1.70 | 3.17 |
| enemy 150-420 m (161 s) | 0.01 | 0.16 | 0.03 | 0.02 | 0.04 | 0.71 | 0.68 | 0.01 | 0.01 | 0.01 | 1.77 | 3.35 |
| enemy < 150 m (86 s) | 0.01 | 0.17 | 0.04 | 0.02 | 0.04 | 1.15 | 0.68 | 0.01 | 0.01 | 0.01 | 2.08 | 4.13 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 10 | 11 | 13 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 238 | 266 | 1347 |
| cell_items | 0 | 21718 | 94459 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 1 | 2 |
| tracer_records | 0 | 0 | 0 |
| events | 1 | 3 | 3 |
| prepare_ms | 0.45 | 0.54 | 0.82 |
| encode_ms | 2.58 | 3.56 | 18.24 |
| point lights offered | 10 | 11 | 12 |
| live instances | 34039 | 34213 | 35809 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.456417083740234 |
| first_main_gun_shot_s | 34.507198333740234 |
| first_hit_taken_s | 37.53275680541992 |
| main_gun_shots | 9 |
| hits_dealt | 6 |
| kills | 6 |
| hits_taken | 4 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2448.1357421875 |
| modules_out | [] |
| reload_s | 9 reloads; median 5.1 s, max 7.1 s |
| stalls | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.2 s: [2, 'Phase line Anton: the hamlet']
- 112.3 s: [3, 'The gun line at the fork']
- 159.7 s: None

