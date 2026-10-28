### Run 1 — seed 29, pilot b6705fe (stalled at 57.6 %: ready rack empty, reload froze); heavy sibling GPU load

Session log `session-20261027-191353.jsonl.gz`: 1261 one-second lines, 1260 with the Canyon clock running.

Internal grid [800, 450], surface [1280, 720]. 49960 frames over 1260 s.

| metric | value |
|---|---|
| mean frame time (exact) | 25.66 ms (39.0 fps) |
| median second fps / 5th-percentile second fps | 39.1 / 17.9 |
| p50 of per-second p50 | 16.45 ms |
| p95 of per-second p95 | 115.40 ms |
| p99 of per-second p99 | 306.59 ms |
| worst single frame | 854.9 ms |
| seconds with p99 > 33.3 ms | 1140 |
| seconds with p50 > 16.7 ms | 555 |
| CPU simulation mean / p99-of-p99 | 13.88 / 113.36 ms |
| CPU render submission mean / p99-of-p99 | 5.48 / 16.02 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 6.30 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.1 | 16.63 | 19.83 | 28.28 | 854.9 | 4.32 | 8.11 | 0.58 | 12 |
| 1 | 52.3 | 16.85 | 30.44 | 74.77 | 151.8 | 10.71 | 7.61 | 0.58 | 59 |
| 2 | 45.8 | 16.35 | 68.23 | 237.83 | 345.9 | 11.59 | 6.42 | 0.58 | 59 |
| 3 | 32.8 | 24.91 | 112.91 | 256.36 | 416.5 | 17.27 | 7.91 | 0.58 | 35 |
| 4 | 37.5 | 16.51 | 115.49 | 236.04 | 237.3 | 12.92 | 5.09 | 0.49 | 55 |
| 5 | 42.0 | 14.19 | 123.25 | 325.38 | 332.1 | 7.27 | 4.27 | 0.11 | 0 |
| 6 | 40.8 | 13.90 | 121.74 | 218.22 | 334.9 | 7.90 | 4.06 | 0.10 | 0 |
| 7 | 44.2 | 14.10 | 103.83 | 205.72 | 278.9 | 6.78 | 3.73 | 0.10 | 0 |
| 8 | 43.3 | 14.17 | 113.68 | 166.42 | 213.9 | 5.99 | 4.13 | 0.10 | 0 |
| 9 | 44.0 | 14.94 | 104.19 | 172.43 | 213.5 | 7.13 | 4.75 | 0.10 | 0 |
| 10 | 44.5 | 14.55 | 99.28 | 204.37 | 228.9 | 6.08 | 4.41 | 0.27 | 0 |
| 11 | 42.8 | 14.12 | 114.32 | 170.54 | 187.2 | 6.34 | 4.67 | 0.58 | 0 |
| 12 | 40.8 | 15.48 | 116.34 | 216.27 | 326.5 | 9.03 | 5.62 | 0.58 | 17 |
| 13 | 36.0 | 17.52 | 128.70 | 275.12 | 295.7 | 14.50 | 6.22 | 0.58 | 46 |
| 14 | 26.3 | 26.21 | 123.21 | 370.42 | 428.0 | 26.81 | 6.18 | 0.58 | 57 |
| 15 | 19.9 | 50.84 | 199.36 | 345.03 | 357.0 | 42.55 | 5.53 | 0.58 | 58 |
| 16 | 21.9 | 48.94 | 95.55 | 305.86 | 357.2 | 36.99 | 5.74 | 0.58 | 57 |
| 17 | 33.5 | 21.17 | 93.61 | 255.75 | 279.3 | 17.47 | 5.22 | 0.58 | 58 |
| 18 | 35.4 | 17.11 | 101.56 | 245.23 | 255.6 | 14.36 | 5.42 | 0.58 | 58 |
| 19 | 37.4 | 16.31 | 90.05 | 234.02 | 294.1 | 13.77 | 5.03 | 0.48 | 58 |
| 20 | 37.9 | 15.88 | 94.58 | 226.39 | 241.4 | 12.88 | 4.88 | 0.11 | 57 |
| 21 | 37.8 | 15.83 | 99.19 | 295.43 | 295.4 | 13.12 | 5.39 | 0.10 | 47 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 863 s | 8.8 | 107.3 / 428.0 / 428.0 | 51.30 / 121.72 | 6.56 / 8.45 | 30.91 | enemy 218 m; 10 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |
| 217 s | 9.4 | 98.5 / 416.5 / 416.5 | 22.68 / 81.74 | 9.77 / 15.23 | 7.67 | enemy 451 m; 6 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Loader |
| 872 s | 19.3 | 65.0 / 370.4 / 370.4 | 32.71 / 97.62 | 4.67 / 7.24 | — | enemy 171 m; 10 live AFV; rm events 2, sparks 2, parcels 13, lights 7; lights offered 7; storm 0.58; station Loader |
| 969 s | 27.4 | 45.2 / 357.2 / 357.2 | 29.61 / 107.26 | 4.50 / 5.54 | — | enemy 50 m; 7 live AFV; rm events 2, sparks 2, parcels 15, lights 8; lights offered 7; storm 0.58; station Loader |
| 941 s | 8.4 | 111.9 / 357.0 / 357.0 | 82.82 / 156.17 | 6.85 / 8.28 | 28.01 | enemy 130 m; 8 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |

GPU passes from 627 fenced frames (pass sum: median 7.56 ms, mean 9.73 ms; fenced frame wall median 44.07 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-sky` | 2.199 | 29.1 % | 2.859 | 4.138 | 11.224 | 627/627 |
| 2 | `rm-gbuffer` | 1.209 | 16.0 % | 1.932 | 2.350 | 128.562 | 627/627 |
| 3 | `rm-shadow-far` | 0.952 | 12.6 % | 1.111 | 2.019 | 22.614 | 627/627 |
| 4 | `rm-point-face` | 0.886 | 11.7 % | 1.097 | 2.081 | 12.978 | 627/627 |
| 5 | `rm-sky-cover` | 0.452 | 6.0 % | 0.777 | 1.096 | 26.370 | 627/627 |
| 6 | `rm-shadow-near` | 0.446 | 5.9 % | 1.207 | 1.091 | 247.914 | 627/627 |
| 7 | `rm-shade` | 0.289 | 3.8 % | 0.306 | 0.428 | 0.589 | 627/627 |
| 8 | `rm-volume` | 0.137 | 1.8 % | 0.229 | 0.543 | 38.141 | 627/627 |
| 9 | `rm-integrate` | 0.045 | 0.6 % | 0.047 | 0.085 | 0.238 | 627/627 |
| 10 | `exposure` | 0.037 | 0.5 % | 0.039 | 0.074 | 0.076 | 627/627 |
| 11 | `bloom-glare` | 0.020 | 0.3 % | 0.022 | 0.040 | 0.041 | 627/627 |
| 12 | `rm-reflect` | 0.016 | 0.2 % | 0.018 | 0.026 | 0.037 | 627/627 |
| 13 | `bloom-composite` | 0.015 | 0.2 % | 0.016 | 0.029 | 0.030 | 627/627 |
| 14 | `bloom-glare-composite` | 0.014 | 0.2 % | 0.015 | 0.027 | 0.029 | 627/627 |
| 15 | `rm-glass` | 0.009 | 0.1 % | 0.007 | 0.011 | 0.013 | 615/627 |
| 16 | `bloom-bright` | 0.008 | 0.1 % | 0.009 | 0.015 | 0.017 | 627/627 |
| 17 | `crush` | 0.008 | 0.1 % | 0.008 | 0.016 | 0.023 | 627/627 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.013 | 0.019 | 627/627 |
| 19 | `bloom-blur-v` | 0.005 | 0.1 % | 0.005 | 0.010 | 0.011 | 627/627 |
| 20 | `bloom-blur-h` | 0.005 | 0.1 % | 0.005 | 0.009 | 0.010 | 627/627 |
| 21 | `rm-points` | 0.003 | 0.0 % | 0.009 | 0.009 | 0.487 | 627/627 |
| 22 | `rm-lens` | 0.000 | 0.0 % | 0.000 | 0.007 | 0.007 | 36/627 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|
| all (1260 s) | 0.01 | 7.14 | 2.39 | 1.07 | 0.05 | 0.03 | 0.01 | 3.18 | 13.88 |
| enemy > 420 m (527 s) | 0.01 | 1.43 | 2.31 | 0.97 | 0.00 | 0.02 | 0.01 | 2.71 | 7.48 |
| enemy 150-420 m (265 s) | 0.01 | 8.65 | 2.19 | 1.15 | 0.01 | 0.03 | 0.01 | 3.01 | 15.07 |
| enemy < 150 m (468 s) | 0.01 | 12.73 | 2.58 | 1.12 | 0.12 | 0.04 | 0.01 | 3.80 | 20.41 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 6 | 8 | 12 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 0 | 6 | 1322 |
| cell_items | 0 | 1152 | 13398 |
| embers | 0 | 0 | 1397 |
| spark_records | 0 | 1 | 5 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 3 | 6 |
| prepare_ms | 0.60 | 0.92 | 1.50 |
| encode_ms | 1.15 | 1.91 | 2.74 |
| point lights offered | 6 | 7 | 8 |
| live instances | 34546 | 35671 | 36652 |

Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):

| | start | median | peak |
|---|---|---|---|
| working set | 105 MB | 2855 MB | 7623 MB |
| private bytes | 85 MB | 7610 MB | 15359 MB |
| board VRAM used | 15550 MB | 22892 MB | 23072 MB |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.51991081237793 |
| first_main_gun_shot_s | 33.564979553222656 |
| first_hit_taken_s | 908.9979248046875 |
| main_gun_shots | 29 |
| hits_dealt | 20 |
| kills | 14 |
| hits_taken | 7 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 1399.64111328125 |
| modules_out | [] |
| reload_s | 28 reloads; median 6.2 s, max 7.7 s |
| stalls | [{'from_s': 603.8629150390625, 'to_s': 665.856689453125, 'at': [-36.39185333251953, -6.6768646240234375, -632.8027954101562]}] |
| tank_stopped_lines | 0 |
| autopilot_result | {'outcome': 'pilot_stalled', 'play_s': 1307.7, 'progress': 0.5758, 'reloads': 28} |
| after_action | None |
| stage_timeouts | 0 |
| panics | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.2 s: [2, 'Phase line Anton: the hamlet']
- 90.1 s: [3, 'The gun line at the fork']
- 107.3 s: None
- 716.7 s: [4, 'The factory yard']
- 778.6 s: None

