### Run C — seed 101, pilot 5ffd352 (stalled at 62.6 %: live T-34 unseen 19 m up the street; first 2.5 min shared with two aborted parallel runs)

Session log `session-20261027-201410.jsonl.gz`: 1139 one-second lines, 1138 with the Canyon clock running.

Internal grid [800, 450], surface [1280, 720]. 60825 frames over 1138 s.

| metric | value |
|---|---|
| mean frame time (exact) | 18.99 ms (52.7 fps) |
| median second fps / 5th-percentile second fps | 58.8 / 36.7 |
| p50 of per-second p50 | 16.71 ms |
| p95 of per-second p95 | 70.51 ms |
| p99 of per-second p99 | 388.50 ms |
| worst single frame | 11692.0 ms |
| seconds with p99 > 33.3 ms | 254 |
| seconds with p50 > 16.7 ms | 588 |
| CPU simulation mean / p99-of-p99 | 9.70 / 38.05 ms |
| CPU render submission mean / p99-of-p99 | 4.83 / 12.56 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 4.46 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 15.1 | 64.08 | 460.95 | 570.15 | 11692.0 | 8.01 | 4.90 | 0.40 | 41 |
| 1 | 46.5 | 17.11 | 70.51 | 134.51 | 203.4 | 10.31 | 4.86 | 0.10 | 59 |
| 2 | 49.3 | 14.89 | 71.01 | 130.50 | 204.8 | 7.42 | 5.47 | 0.10 | 41 |
| 3 | 58.0 | 16.56 | 71.23 | 95.75 | 96.4 | 5.19 | 4.58 | 0.10 | 20 |
| 4 | 60.0 | 16.35 | 21.49 | 27.53 | 69.8 | 10.15 | 4.46 | 0.10 | 60 |
| 5 | 60.0 | 16.36 | 21.01 | 29.65 | 33.5 | 9.51 | 4.25 | 0.10 | 48 |
| 6 | 60.0 | 16.67 | 21.41 | 26.87 | 33.2 | 7.01 | 5.04 | 0.36 | 0 |
| 7 | 60.0 | 16.63 | 21.05 | 26.52 | 31.8 | 6.95 | 4.78 | 0.58 | 0 |
| 8 | 60.0 | 16.69 | 19.80 | 21.70 | 28.5 | 5.39 | 4.14 | 0.58 | 0 |
| 9 | 60.0 | 16.67 | 19.99 | 21.83 | 29.8 | 5.19 | 4.19 | 0.58 | 0 |
| 10 | 60.0 | 16.69 | 20.67 | 22.86 | 45.9 | 5.16 | 4.67 | 0.58 | 0 |
| 11 | 60.0 | 16.65 | 20.17 | 22.10 | 30.8 | 5.71 | 4.73 | 0.58 | 0 |
| 12 | 57.0 | 16.70 | 27.93 | 37.01 | 63.5 | 8.41 | 5.66 | 0.58 | 34 |
| 13 | 58.2 | 16.22 | 25.79 | 46.32 | 198.0 | 11.60 | 4.72 | 0.58 | 60 |
| 14 | 49.8 | 20.16 | 30.78 | 41.17 | 42.9 | 14.35 | 5.30 | 0.58 | 59 |
| 15 | 45.7 | 20.35 | 33.33 | 42.98 | 44.7 | 16.40 | 5.08 | 0.40 | 59 |
| 16 | 46.3 | 20.41 | 34.18 | 41.16 | 42.1 | 16.40 | 5.07 | 0.10 | 60 |
| 17 | 51.0 | 18.35 | 29.20 | 40.72 | 42.1 | 14.41 | 4.78 | 0.10 | 59 |
| 18 | 48.0 | 19.10 | 31.56 | 39.78 | 41.0 | 15.29 | 5.19 | 0.10 | 59 |
| 19 | 54.1 | 18.15 | 24.38 | 26.46 | 30.8 | 13.59 | 4.32 | 0.10 | 18 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 1 s | 9.9 | 1160.8 / 11692.0 / 11692.0 | 7.63 / 29.56 | 5.37 / 14.30 | — | enemy 483 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.58; station Commander; beat 1 'Read the ground' |
| 29 s | 8.9 | 172.9 / 570.1 / 570.1 | 5.64 / 12.18 | 7.25 / 8.02 | — | enemy 275 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 7; lights offered 7; storm 0.40; station Gunner; optic; beat 2 'Phase line Anton: the hamlet' |
| 27 s | 8.3 | 179.5 / 551.9 / 551.9 | 7.34 / 17.55 | 6.68 / 7.42 | — | enemy 280 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 7; lights offered 7; storm 0.42; station Gunner; optic; beat 2 'Phase line Anton: the hamlet' |
| 24 s | 10.9 | 130.4 / 461.0 / 461.0 | 9.48 / 28.44 | 4.28 / 4.53 | — | enemy 311 m; 11 live AFV; rm events 0, sparks 0, parcels 0, lights 7; lights offered 7; storm 0.47; station Commander; beat 2 'Phase line Anton: the hamlet' |
| 39 s | 17.5 | 79.7 / 455.2 / 455.2 | 6.82 / 21.32 | 3.61 / 4.35 | — | enemy 248 m; 11 live AFV; rm events 1, sparks 0, parcels 0, lights 6; lights offered 6; storm 0.26; station Loader; beat 2 'Phase line Anton: the hamlet' |

GPU passes from 568 fenced frames (pass sum: median 7.17 ms, mean 10.14 ms; fenced frame wall median 15.13 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-sky` | 2.341 | 32.7 % | 2.884 | 4.133 | 11.240 | 568/568 |
| 2 | `rm-gbuffer` | 1.122 | 15.7 % | 2.239 | 1.948 | 183.383 | 568/568 |
| 3 | `rm-point-face` | 0.999 | 13.9 % | 1.453 | 1.622 | 116.409 | 568/568 |
| 4 | `rm-shadow-far` | 0.811 | 11.3 % | 1.277 | 1.809 | 49.874 | 568/568 |
| 5 | `rm-sky-cover` | 0.461 | 6.4 % | 0.979 | 0.807 | 105.503 | 568/568 |
| 6 | `rm-shadow-near` | 0.412 | 5.7 % | 0.602 | 0.903 | 76.848 | 568/568 |
| 7 | `rm-shade` | 0.302 | 4.2 % | 0.312 | 0.428 | 0.674 | 568/568 |
| 8 | `rm-volume` | 0.125 | 1.7 % | 0.193 | 0.557 | 18.400 | 568/568 |
| 9 | `rm-integrate` | 0.046 | 0.6 % | 0.048 | 0.051 | 0.096 | 568/568 |
| 10 | `exposure` | 0.038 | 0.5 % | 0.039 | 0.044 | 0.076 | 568/568 |
| 11 | `bloom-glare` | 0.020 | 0.3 % | 0.021 | 0.022 | 0.043 | 568/568 |
| 12 | `rm-reflect` | 0.015 | 0.2 % | 0.016 | 0.024 | 0.034 | 568/568 |
| 13 | `bloom-composite` | 0.015 | 0.2 % | 0.016 | 0.015 | 0.030 | 568/568 |
| 14 | `bloom-glare-composite` | 0.014 | 0.2 % | 0.015 | 0.014 | 0.029 | 568/568 |
| 15 | `rm-glass` | 0.010 | 0.1 % | 0.009 | 0.011 | 0.019 | 556/568 |
| 16 | `bloom-bright` | 0.009 | 0.1 % | 0.009 | 0.009 | 0.017 | 568/568 |
| 17 | `crush` | 0.008 | 0.1 % | 0.008 | 0.016 | 0.033 | 568/568 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.010 | 0.019 | 568/568 |
| 19 | `bloom-blur-h` | 0.005 | 0.1 % | 0.006 | 0.008 | 0.229 | 568/568 |
| 20 | `bloom-blur-v` | 0.005 | 0.1 % | 0.005 | 0.006 | 0.012 | 568/568 |
| 21 | `rm-points` | 0.003 | 0.0 % | 0.005 | 0.008 | 0.204 | 568/568 |
| 22 | `rm-lens` | 0.000 | 0.0 % | 0.000 | 0.007 | 0.007 | 28/568 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (1138 s) | 0.01 | 4.38 | 0.09 | 4.23 | 0.02 | 1.89 | 0.95 | 0.06 | 0.02 | 0.01 | 2.38 | 9.70 |
| enemy > 420 m (461 s) | 0.01 | 0.92 | 0.09 | 0.79 | 0.01 | 1.86 | 0.90 | 0.00 | 0.02 | 0.01 | 2.18 | 5.89 |
| enemy 150-420 m (313 s) | 0.01 | 4.52 | 0.08 | 4.37 | 0.04 | 1.78 | 1.03 | 0.20 | 0.02 | 0.01 | 2.35 | 9.94 |
| enemy < 150 m (364 s) | 0.01 | 8.63 | 0.11 | 8.46 | 0.04 | 2.02 | 0.95 | 0.00 | 0.02 | 0.01 | 2.66 | 14.31 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 6 | 8 | 10 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 0 | 5 | 1297 |
| cell_items | 0 | 60 | 10944 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 0 | 4 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 2 | 5 |
| prepare_ms | 0.58 | 0.82 | 1.18 |
| encode_ms | 1.11 | 1.72 | 2.30 |
| point lights offered | 6 | 7 | 8 |
| live instances | 34492 | 35502 | 36576 |

Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):

| | start | median | peak |
|---|---|---|---|
| working set | 98 MB | 2874 MB | 7911 MB |
| private bytes | 130 MB | 7656 MB | 15325 MB |
| board VRAM used | 12315 MB | 12036 MB | 23032 MB |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 12.643199920654297 |
| first_main_gun_shot_s | 30.906213760375977 |
| first_hit_taken_s | 32.60283279418945 |
| main_gun_shots | 17 |
| hits_dealt | 11 |
| kills | 11 |
| hits_taken | 4 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 1370.9754638671875 |
| modules_out | [] |
| reload_s | 17 reloads; median 6.1 s, max 7.1 s |
| stalls | [{'from_s': 624.4134521484375, 'to_s': 646.6297607421875, 'at': [-42.55362319946289, -5.781347274780273, -638.9844970703125]}] |
| tank_stopped_lines | 0 |
| autopilot_result | {'outcome': 'pilot_stalled', 'play_s': 1151.7, 'progress': 0.6262, 'reloads': 17} |
| after_action | None |
| stage_timeouts | 0 |
| panics | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.2 s: [2, 'Phase line Anton: the hamlet']
- 84.3 s: [3, 'The gun line at the fork']
- 122.9 s: None
- 680.9 s: [4, 'The factory yard']
- 771.9 s: None

