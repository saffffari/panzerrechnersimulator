### Run 0 — seed 29, merged pilot 3f1c693 before the hull bypass (vsync on; jammed on dead T-34s in the hamlet, froze on focus loss at 310 s); heavy sibling GPU load

Session log `session-20261027-184432.jsonl.gz`: 307 one-second lines, 305 with the Canyon clock running.

Internal grid [800, 450], surface [1280, 720]. 11150 frames over 305 s.

| metric | value |
|---|---|
| mean frame time (exact) | 27.67 ms (36.1 fps) |
| median second fps / 5th-percentile second fps | 34.0 / 15.5 |
| p50 of per-second p50 | 18.67 ms |
| p95 of per-second p95 | 197.58 ms |
| p99 of per-second p99 | 487.02 ms |
| worst single frame | 1389.3 ms |
| seconds with p99 > 33.3 ms | 259 |
| seconds with p50 > 16.7 ms | 243 |
| CPU simulation mean / p99-of-p99 | 13.32 / 166.11 ms |
| CPU render submission mean / p99-of-p99 | 6.59 / 18.63 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 7.76 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 44.5 | 16.78 | 156.54 | 223.67 | 1389.3 | 3.38 | 5.39 | 0.58 | 13 |
| 1 | 35.9 | 19.45 | 185.36 | 308.81 | 308.8 | 14.40 | 7.13 | 0.58 | 58 |
| 2 | 36.6 | 18.75 | 197.58 | 321.29 | 374.3 | 14.06 | 6.50 | 0.58 | 57 |
| 3 | 29.8 | 24.74 | 199.13 | 343.84 | 349.2 | 18.12 | 7.94 | 0.58 | 56 |
| 4 | 32.5 | 21.38 | 179.37 | 487.02 | 519.9 | 19.63 | 6.65 | 0.53 | 53 |
| 5 | 37.4 | 16.77 | 289.07 | 523.58 | 523.6 | 7.79 | 5.02 | 0.20 | 3 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 312 s | 1.2 | 95.1 / 523.6 / 523.6 | 9.29 / 15.82 | 4.98 / 6.88 | 39.79 | enemy 1108 m; 5 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.11; station Commander |
| 288 s | 4.8 | 228.3 / 519.9 / 519.9 | 31.27 / 56.47 | 4.41 / 4.64 | — | enemy 68 m; 6 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.39; station Commander |
| 313 s | 9.9 | 106.0 / 512.2 / 512.2 | 10.69 / 27.24 | 5.11 / 7.34 | — | enemy 1109 m; 5 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.11; station Commander |
| 268 s | 1.5 | 249.8 / 487.0 / 487.0 | 85.63 / 167.57 | 5.81 / 6.21 | 9.94 | enemy 66 m; 6 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |
| 267 s | 4.6 | 257.2 / 432.9 / 432.9 | 30.02 / 86.43 | 6.71 / 8.01 | — | enemy 66 m; 6 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander |

GPU passes from 155 fenced frames (pass sum: median 7.72 ms, mean 12.06 ms; fenced frame wall median 26.21 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-sky` | 2.107 | 27.3 % | 2.704 | 4.232 | 10.710 | 155/155 |
| 2 | `rm-gbuffer` | 1.541 | 20.0 % | 2.420 | 2.679 | 72.285 | 155/155 |
| 3 | `rm-point-face` | 1.294 | 16.8 % | 2.677 | 3.032 | 148.948 | 155/155 |
| 4 | `rm-shadow-far` | 0.999 | 13.0 % | 1.859 | 2.318 | 57.626 | 155/155 |
| 5 | `rm-sky-cover` | 0.477 | 6.2 % | 0.930 | 1.038 | 30.701 | 155/155 |
| 6 | `rm-shadow-near` | 0.464 | 6.0 % | 0.718 | 1.542 | 17.386 | 155/155 |
| 7 | `rm-shade` | 0.314 | 4.1 % | 0.331 | 0.435 | 0.973 | 155/155 |
| 8 | `rm-volume` | 0.125 | 1.6 % | 0.176 | 0.496 | 1.102 | 155/155 |
| 9 | `rm-integrate` | 0.045 | 0.6 % | 0.049 | 0.088 | 0.147 | 155/155 |
| 10 | `exposure` | 0.037 | 0.5 % | 0.041 | 0.074 | 0.075 | 155/155 |
| 11 | `bloom-glare` | 0.020 | 0.3 % | 0.025 | 0.041 | 0.217 | 155/155 |
| 12 | `rm-reflect` | 0.016 | 0.2 % | 0.024 | 0.030 | 1.048 | 155/155 |
| 13 | `bloom-composite` | 0.015 | 0.2 % | 0.021 | 0.030 | 0.728 | 155/155 |
| 14 | `bloom-glare-composite` | 0.014 | 0.2 % | 0.015 | 0.029 | 0.029 | 155/155 |
| 15 | `rm-glass` | 0.010 | 0.1 % | 0.008 | 0.011 | 0.017 | 148/155 |
| 16 | `bloom-bright` | 0.009 | 0.1 % | 0.022 | 0.016 | 1.269 | 155/155 |
| 17 | `crush` | 0.008 | 0.1 % | 0.009 | 0.016 | 0.018 | 155/155 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.009 | 0.014 | 0.126 | 155/155 |
| 19 | `bloom-blur-h` | 0.005 | 0.1 % | 0.009 | 0.010 | 0.333 | 155/155 |
| 20 | `bloom-blur-v` | 0.005 | 0.1 % | 0.008 | 0.010 | 0.207 | 155/155 |
| 21 | `rm-points` | 0.004 | 0.1 % | 0.005 | 0.008 | 0.116 | 155/155 |
| 22 | `rm-lens` | 0.000 | 0.0 % | 0.001 | 0.007 | 0.007 | 14/155 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 6 | 8 | 9 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 0 | 6 | 1307 |
| cell_items | 0 | 6912 | 10944 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 0 | 3 |
| tracer_records | 0 | 0 | 0 |
| events | 1 | 2 | 3 |
| prepare_ms | 0.65 | 0.99 | 1.37 |
| encode_ms | 1.27 | 2.19 | 3.34 |
| point lights offered | 6 | 7 | 7 |
| live instances | 33884 | 34072 | 35340 |

Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):

| | start | median | peak |
|---|---|---|---|
| working set | 3113 MB | 3824 MB | 7101 MB |
| private bytes | 5969 MB | 7474 MB | 15116 MB |
| board VRAM used | 22625 MB | 22662 MB | 22922 MB |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.845727920532227 |
| first_main_gun_shot_s | 32.86228561401367 |
| first_hit_taken_s | None |
| main_gun_shots | 11 |
| hits_dealt | 7 |
| kills | 6 |
| hits_taken | 0 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2457.103271484375 |
| modules_out | [] |
| reload_s | 11 reloads; median 6.1 s, max 7.3 s |
| stalls | [] |
| tank_stopped_lines | 0 |
| autopilot_result | None |
| after_action | None |
| stage_timeouts | 0 |
| panics | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.2 s: [2, 'Phase line Anton: the hamlet']
- 90.2 s: [3, 'The gun line at the fork']
- 110.5 s: None

