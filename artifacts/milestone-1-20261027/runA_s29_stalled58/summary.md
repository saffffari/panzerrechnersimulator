### Run A — seed 29, pilot 7804cf8 (stalled at 57.9 %: escape heading dithered among hulls)

Session log `session-20261027-205624.jsonl.gz`: 1153 one-second lines, 1152 with the Canyon clock running.

Internal grid [800, 450], surface [1280, 720]. 67824 frames over 1152 s.

| metric | value |
|---|---|
| mean frame time (exact) | 17.02 ms (58.8 fps) |
| median second fps / 5th-percentile second fps | 59.0 / 50.1 |
| p50 of per-second p50 | 16.62 ms |
| p95 of per-second p95 | 24.82 ms |
| p99 of per-second p99 | 39.11 ms |
| worst single frame | 587.6 ms |
| seconds with p99 > 33.3 ms | 34 |
| seconds with p50 > 16.7 ms | 384 |
| CPU simulation mean / p99-of-p99 | 8.60 / 28.80 ms |
| CPU render submission mean / p99-of-p99 | 4.73 / 8.93 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 3.68 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.5 | 16.66 | 19.59 | 20.38 | 587.6 | 2.57 | 4.57 | 0.58 | 13 |
| 1 | 60.0 | 16.69 | 19.56 | 29.07 | 44.3 | 7.95 | 4.64 | 0.58 | 59 |
| 2 | 59.9 | 16.64 | 19.81 | 23.19 | 93.8 | 9.29 | 4.66 | 0.58 | 60 |
| 3 | 54.2 | 19.01 | 25.88 | 39.11 | 263.7 | 12.08 | 4.84 | 0.58 | 40 |
| 4 | 57.3 | 16.48 | 27.82 | 36.53 | 39.2 | 11.21 | 5.13 | 0.49 | 59 |
| 5 | 60.0 | 16.68 | 19.81 | 24.32 | 38.1 | 6.44 | 4.28 | 0.11 | 5 |
| 6 | 59.8 | 16.65 | 20.29 | 26.58 | 31.2 | 6.91 | 4.96 | 0.10 | 0 |
| 7 | 60.0 | 16.69 | 20.53 | 24.74 | 38.9 | 6.08 | 4.51 | 0.10 | 0 |
| 8 | 60.0 | 16.65 | 19.79 | 21.07 | 33.3 | 4.75 | 3.76 | 0.10 | 0 |
| 9 | 60.0 | 16.66 | 20.11 | 27.11 | 43.6 | 5.69 | 4.61 | 0.10 | 0 |
| 10 | 60.0 | 16.70 | 20.71 | 24.98 | 36.2 | 4.82 | 4.71 | 0.28 | 0 |
| 11 | 60.0 | 16.71 | 19.64 | 23.92 | 34.1 | 5.83 | 4.62 | 0.58 | 4 |
| 12 | 55.4 | 16.60 | 31.15 | 43.50 | 46.4 | 10.72 | 5.44 | 0.58 | 35 |
| 13 | 59.5 | 16.03 | 22.85 | 35.52 | 274.4 | 10.92 | 4.72 | 0.58 | 60 |
| 14 | 55.7 | 16.50 | 29.30 | 53.90 | 163.8 | 11.92 | 5.45 | 0.58 | 59 |
| 15 | 58.1 | 16.48 | 23.89 | 26.87 | 36.2 | 11.79 | 4.84 | 0.58 | 60 |
| 16 | 59.6 | 16.09 | 22.93 | 25.65 | 35.0 | 11.65 | 4.56 | 0.58 | 59 |
| 17 | 56.7 | 16.38 | 29.40 | 35.60 | 39.7 | 11.82 | 5.12 | 0.58 | 60 |
| 18 | 60.0 | 16.16 | 21.72 | 27.82 | 34.0 | 10.38 | 4.61 | 0.58 | 59 |
| 19 | 60.0 | 16.26 | 21.48 | 22.44 | 34.3 | 10.42 | 4.57 | 0.58 | 23 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 808 s | 39.6 | 24.8 / 274.4 / 274.4 | 18.82 / 268.90 | 4.92 / 7.51 | 6.43 | enemy 168 m; 10 live AFV; rm events 1, sparks 2, parcels 2, lights 7; lights offered 6; storm 0.58; station Gunner; optic |
| 895 s | 30.3 | 32.3 / 163.8 / 163.8 | 19.92 / 154.27 | 11.15 / 13.50 | 15.87 | enemy 29 m; 7 live AFV; rm events 1, sparks 0, parcels 5, lights 6; lights offered 6; storm 0.58; station Gunner; optic |
| 896 s | 40.8 | 24.7 / 53.9 / 53.9 | 16.96 / 41.65 | 6.98 / 12.68 | — | enemy 136 m; 6 live AFV; rm events 2, sparks 0, parcels 11, lights 8; lights offered 7; storm 0.58; station Loader |
| 892 s | 37.6 | 25.8 / 46.8 / 46.8 | 12.46 / 20.41 | 12.29 / 15.81 | 12.93 | enemy 29 m; 7 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Gunner; optic |
| 742 s | 32.7 | 29.4 / 46.4 / 46.4 | 20.57 / 36.97 | 8.01 / 9.13 | 7.02 | enemy 443 m; 12 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander; beat 4 'The factory yard' |

GPU passes from 576 fenced frames (pass sum: median 7.43 ms, mean 7.73 ms; fenced frame wall median 15.03 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-sky` | 3.401 | 45.8 % | 3.037 | 4.178 | 10.780 | 576/576 |
| 2 | `rm-gbuffer` | 1.264 | 17.0 % | 1.263 | 1.881 | 2.527 | 576/576 |
| 3 | `rm-point-face` | 0.908 | 12.2 % | 0.908 | 1.360 | 3.665 | 576/576 |
| 4 | `rm-shadow-far` | 0.896 | 12.1 % | 0.959 | 1.420 | 2.708 | 576/576 |
| 5 | `rm-sky-cover` | 0.450 | 6.0 % | 0.456 | 0.699 | 2.676 | 576/576 |
| 6 | `rm-shadow-near` | 0.423 | 5.7 % | 0.435 | 0.653 | 1.114 | 576/576 |
| 7 | `rm-shade` | 0.302 | 4.1 % | 0.311 | 0.425 | 0.593 | 576/576 |
| 8 | `rm-volume` | 0.124 | 1.7 % | 0.154 | 0.455 | 1.256 | 576/576 |
| 9 | `rm-integrate` | 0.046 | 0.6 % | 0.049 | 0.087 | 0.260 | 576/576 |
| 10 | `exposure` | 0.037 | 0.5 % | 0.039 | 0.038 | 0.077 | 576/576 |
| 11 | `bloom-glare` | 0.020 | 0.3 % | 0.021 | 0.021 | 0.042 | 576/576 |
| 12 | `rm-reflect` | 0.015 | 0.2 % | 0.016 | 0.023 | 0.034 | 576/576 |
| 13 | `bloom-composite` | 0.015 | 0.2 % | 0.016 | 0.015 | 0.030 | 576/576 |
| 14 | `bloom-glare-composite` | 0.014 | 0.2 % | 0.015 | 0.015 | 0.029 | 576/576 |
| 15 | `rm-glass` | 0.010 | 0.1 % | 0.009 | 0.011 | 0.012 | 567/576 |
| 16 | `bloom-bright` | 0.008 | 0.1 % | 0.009 | 0.009 | 0.017 | 576/576 |
| 17 | `crush` | 0.008 | 0.1 % | 0.008 | 0.009 | 0.018 | 576/576 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.010 | 0.017 | 576/576 |
| 19 | `bloom-blur-v` | 0.005 | 0.1 % | 0.005 | 0.006 | 0.011 | 576/576 |
| 20 | `bloom-blur-h` | 0.005 | 0.1 % | 0.005 | 0.005 | 0.011 | 576/576 |
| 21 | `rm-points` | 0.003 | 0.0 % | 0.005 | 0.008 | 0.209 | 576/576 |
| 22 | `rm-lens` | 0.000 | 0.0 % | 0.000 | 0.007 | 0.007 | 29/576 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (1152 s) | 0.01 | 3.58 | 0.10 | 3.41 | 0.03 | 1.87 | 0.79 | 0.02 | 0.02 | 0.01 | 2.32 | 8.60 |
| enemy > 420 m (497 s) | 0.01 | 1.18 | 0.12 | 1.02 | 0.01 | 1.78 | 0.80 | 0.00 | 0.02 | 0.01 | 2.07 | 5.87 |
| enemy 150-420 m (313 s) | 0.01 | 5.52 | 0.07 | 5.37 | 0.04 | 1.62 | 0.78 | 0.02 | 0.02 | 0.01 | 2.16 | 10.14 |
| enemy < 150 m (342 s) | 0.01 | 5.28 | 0.10 | 5.10 | 0.05 | 2.22 | 0.77 | 0.03 | 0.03 | 0.01 | 2.82 | 11.16 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 6 | 8 | 10 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 0 | 5 | 1228 |
| cell_items | 0 | 4 | 11520 |
| embers | 0 | 0 | 1397 |
| spark_records | 0 | 0 | 4 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 2 | 4 |
| prepare_ms | 0.56 | 0.77 | 1.09 |
| encode_ms | 1.13 | 1.42 | 2.28 |
| point lights offered | 6 | 7 | 8 |
| live instances | 34539 | 35483 | 36484 |

Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):

| | start | median | peak |
|---|---|---|---|
| working set | 525 MB | 2633 MB | 6916 MB |
| private bytes | 581 MB | 7554 MB | 15336 MB |
| board VRAM used | 7684 MB | 12044 MB | 17336 MB |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.528852462768555 |
| first_main_gun_shot_s | 33.59337615966797 |
| first_hit_taken_s | 185.12643432617188 |
| main_gun_shots | 17 |
| hits_dealt | 13 |
| kills | 13 |
| hits_taken | 5 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 1408.4041748046875 |
| modules_out | [] |
| reload_s | 17 reloads; median 6.1 s, max 7.1 s |
| stalls | [] |
| tank_stopped_lines | 0 |
| autopilot_result | {'outcome': 'pilot_stalled', 'play_s': 1164.0, 'progress': 0.5791, 'reloads': 17} |
| after_action | None |
| stage_timeouts | 0 |
| panics | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.3 s: [2, 'Phase line Anton: the hamlet']
- 90.2 s: [3, 'The gun line at the fork']
- 107.4 s: None
- 677.3 s: [4, 'The factory yard']
- 757.2 s: None

