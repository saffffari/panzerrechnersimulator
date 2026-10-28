### Run B — seed 7, pilot 7804cf8 (knocked out at 44.9 %: one penetration, fuel fire, cook-off)

Session log `session-20261027-203633.jsonl.gz`: 1102 one-second lines, 1101 with the Canyon clock running.

Internal grid [800, 450], surface [1280, 720]. 62521 frames over 1101 s.

| metric | value |
|---|---|
| mean frame time (exact) | 17.63 ms (56.7 fps) |
| median second fps / 5th-percentile second fps | 59.1 / 44.6 |
| p50 of per-second p50 | 16.71 ms |
| p95 of per-second p95 | 27.71 ms |
| p99 of per-second p99 | 44.78 ms |
| worst single frame | 604.7 ms |
| seconds with p99 > 33.3 ms | 61 |
| seconds with p50 > 16.7 ms | 567 |
| CPU simulation mean / p99-of-p99 | 8.60 / 36.95 ms |
| CPU render submission mean / p99-of-p99 | 4.48 / 9.41 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 4.55 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.4 | 16.62 | 20.18 | 22.84 | 604.7 | 3.10 | 5.03 | 0.58 | 39 |
| 1 | 60.0 | 16.60 | 19.69 | 24.15 | 44.6 | 8.64 | 4.80 | 0.58 | 59 |
| 2 | 59.6 | 16.63 | 20.79 | 30.40 | 75.5 | 8.29 | 4.69 | 0.38 | 48 |
| 3 | 60.0 | 16.68 | 19.75 | 22.72 | 34.8 | 4.83 | 4.49 | 0.10 | 15 |
| 4 | 60.0 | 16.65 | 20.91 | 25.37 | 34.1 | 9.10 | 4.31 | 0.10 | 49 |
| 5 | 59.7 | 16.64 | 21.93 | 30.33 | 40.6 | 6.95 | 4.56 | 0.10 | 4 |
| 6 | 60.0 | 16.67 | 20.19 | 22.68 | 29.2 | 6.40 | 4.32 | 0.10 | 0 |
| 7 | 60.0 | 16.67 | 19.95 | 23.94 | 28.4 | 5.72 | 4.08 | 0.10 | 0 |
| 8 | 60.0 | 16.67 | 20.29 | 21.60 | 23.3 | 4.91 | 4.04 | 0.40 | 0 |
| 9 | 60.0 | 16.67 | 20.03 | 21.20 | 24.1 | 4.99 | 4.16 | 0.58 | 0 |
| 10 | 60.0 | 16.71 | 20.54 | 22.55 | 34.2 | 5.40 | 4.67 | 0.58 | 0 |
| 11 | 60.0 | 16.65 | 21.04 | 22.74 | 34.7 | 5.28 | 4.07 | 0.58 | 0 |
| 12 | 60.0 | 16.67 | 19.64 | 24.84 | 34.0 | 5.11 | 4.95 | 0.58 | 49 |
| 13 | 59.3 | 16.68 | 20.00 | 27.05 | 33.8 | 6.13 | 4.78 | 0.58 | 59 |
| 14 | 44.8 | 20.76 | 33.92 | 60.19 | 60.7 | 16.43 | 4.51 | 0.58 | 59 |
| 15 | 49.6 | 19.87 | 26.91 | 42.59 | 43.9 | 15.14 | 3.84 | 0.58 | 60 |
| 16 | 47.5 | 19.92 | 32.88 | 42.14 | 45.8 | 15.85 | 4.14 | 0.58 | 59 |
| 17 | 46.2 | 20.82 | 32.91 | 44.78 | 47.8 | 15.84 | 4.74 | 0.36 | 59 |
| 18 | 46.2 | 19.44 | 88.37 | 125.59 | 125.6 | 21.33 | 5.23 | 0.10 | 32 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 1109 s | 12.7 | 79.3 / 125.6 / 125.6 | 68.00 / 109.25 | 10.31 / 11.34 | — | enemy 80 m; 9 live AFV; rm events 0, sparks 1, parcels 15873, lights 18; lights offered 18; storm 0.10; station Commander; beat 4 'The factory yard' |
| 1111 s | 12.0 | 84.3 / 93.7 / 93.7 | 71.07 / 81.39 | 11.71 / 12.88 | — | enemy 78 m; 9 live AFV; rm events 0, sparks 0, parcels 15873, lights 18; lights offered 18; storm 0.10; station Commander; beat 4 'The factory yard' |
| 1112 s | 10.7 | 85.4 / 93.6 / 93.6 | 71.41 / 76.35 | 12.24 / 12.91 | 6.95 | enemy 78 m; 9 live AFV; rm events 0, sparks 0, parcels 15873, lights 17; lights offered 17; storm 0.10; station Commander; beat 4 'The factory yard' |
| 1110 s | 11.3 | 81.0 / 87.2 / 87.2 | 68.96 / 75.07 | 11.40 / 11.67 | 7.16 | enemy 78 m; 9 live AFV; rm events 0, sparks 1, parcels 15873, lights 18; lights offered 18; storm 0.10; station Commander; beat 4 'The factory yard' |
| 875 s | 32.5 | 30.8 / 60.7 / 60.7 | 22.32 / 51.80 | 6.56 / 8.65 | — | enemy 300 m; 7 live AFV; rm events 5, sparks 4, parcels 45, lights 7; lights offered 7; storm 0.58; station Loader |

GPU passes from 551 fenced frames (pass sum: median 6.99 ms, mean 7.28 ms; fenced frame wall median 14.08 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-sky` | 3.326 | 47.6 % | 2.994 | 4.272 | 11.220 | 551/551 |
| 2 | `rm-gbuffer` | 1.076 | 15.4 % | 1.081 | 1.927 | 2.370 | 551/551 |
| 3 | `rm-shadow-far` | 0.871 | 12.5 % | 0.908 | 1.480 | 2.037 | 551/551 |
| 4 | `rm-point-face` | 0.587 | 8.4 % | 0.721 | 1.306 | 3.038 | 551/551 |
| 5 | `rm-shadow-near` | 0.387 | 5.5 % | 0.418 | 0.670 | 1.108 | 551/551 |
| 6 | `rm-sky-cover` | 0.380 | 5.4 % | 0.423 | 0.675 | 1.046 | 551/551 |
| 7 | `rm-shade` | 0.323 | 4.6 % | 0.334 | 0.433 | 0.622 | 551/551 |
| 8 | `rm-volume` | 0.136 | 1.9 % | 0.183 | 0.616 | 1.006 | 551/551 |
| 9 | `rm-integrate` | 0.046 | 0.7 % | 0.051 | 0.091 | 0.654 | 551/551 |
| 10 | `exposure` | 0.038 | 0.5 % | 0.041 | 0.074 | 0.254 | 551/551 |
| 11 | `bloom-glare` | 0.020 | 0.3 % | 0.022 | 0.041 | 0.261 | 551/551 |
| 12 | `rm-reflect` | 0.015 | 0.2 % | 0.016 | 0.027 | 0.034 | 551/551 |
| 13 | `bloom-composite` | 0.015 | 0.2 % | 0.016 | 0.030 | 0.030 | 551/551 |
| 14 | `bloom-glare-composite` | 0.014 | 0.2 % | 0.015 | 0.028 | 0.029 | 551/551 |
| 15 | `rm-glass` | 0.009 | 0.1 % | 0.008 | 0.011 | 0.013 | 533/551 |
| 16 | `bloom-bright` | 0.008 | 0.1 % | 0.009 | 0.015 | 0.017 | 551/551 |
| 17 | `crush` | 0.008 | 0.1 % | 0.009 | 0.016 | 0.017 | 551/551 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.014 | 0.017 | 551/551 |
| 19 | `bloom-blur-v` | 0.005 | 0.1 % | 0.005 | 0.010 | 0.012 | 551/551 |
| 20 | `bloom-blur-h` | 0.005 | 0.1 % | 0.005 | 0.010 | 0.010 | 551/551 |
| 21 | `rm-points` | 0.003 | 0.0 % | 0.013 | 0.117 | 0.530 | 551/551 |
| 22 | `rm-lens` | 0.000 | 0.0 % | 0.001 | 0.007 | 0.007 | 44/551 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (1101 s) | 0.01 | 3.65 | 0.11 | 3.48 | 0.02 | 1.83 | 0.84 | 0.12 | 0.02 | 0.02 | 2.13 | 8.60 |
| enemy > 420 m (510 s) | 0.01 | 0.72 | 0.09 | 0.59 | 0.01 | 1.80 | 0.82 | 0.00 | 0.01 | 0.01 | 2.13 | 5.51 |
| enemy 150-420 m (512 s) | 0.01 | 6.02 | 0.12 | 5.82 | 0.04 | 1.90 | 0.86 | 0.01 | 0.02 | 0.01 | 2.15 | 10.97 |
| enemy < 150 m (79 s) | 0.01 | 7.13 | 0.08 | 6.99 | 0.02 | 1.61 | 0.87 | 1.56 | 0.02 | 0.13 | 1.92 | 13.25 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 6 | 7 | 18 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 0 | 6 | 15873 |
| cell_items | 0 | 1367 | 262144 |
| embers | 0 | 48 | 49140 |
| spark_records | 0 | 2 | 4 |
| tracer_records | 0 | 0 | 2 |
| events | 0 | 3 | 5 |
| prepare_ms | 0.57 | 0.70 | 7.98 |
| encode_ms | 0.98 | 1.27 | 1.74 |
| point lights offered | 6 | 7 | 18 |
| live instances | 34647 | 35979 | 99627 |

Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):

| | start | median | peak |
|---|---|---|---|
| working set | 237 MB | 2680 MB | 6821 MB |
| private bytes | 288 MB | 7595 MB | 15104 MB |
| board VRAM used | 7384 MB | 12035 MB | 20273 MB |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 22.4641056060791 |
| first_main_gun_shot_s | 30.52958106994629 |
| first_hit_taken_s | 53.59707260131836 |
| main_gun_shots | 31 |
| hits_dealt | 7 |
| kills | 7 |
| hits_taken | 2 |
| penetrations_taken | 1 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 1767.2459716796875 |
| modules_out | ['Engine', 'Fuel'] |
| reload_s | 30 reloads; median 6.1 s, max 25.3 s |
| stalls | [{'from_s': 641.783203125, 'to_s': 714.5255737304688, 'at': [-90.95808410644531, -3.871039867401123, -626.9522094726562]}] |
| tank_stopped_lines | 0 |
| autopilot_result | {'outcome': 'knocked_out', 'play_s': 1112.3, 'progress': 0.4488, 'reloads': 30} |
| after_action | THE RUN IS OVER — FUEL FIRE — BURNED OUT; [("TIME", "18:32.2"), ("REACHED", "2213 m of 4926 m  (45%)"), ("BEST", "no finish yet"), ("FIRE", "31 rounds fired, 7 kills"), ("HITS TAKEN", "2 (1 through the armour)"), ("ENEMY", "16 tanks fielded against you"), ("LOST", "engine, fuel, ammunition rack"), ("ATTEMPT", "seed 7  ·  start 14:48  ·  light snow")] |
| stage_timeouts | 0 |
| panics | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.3 s: [2, 'Phase line Anton: the hamlet']
- 91.1 s: [3, 'The gun line at the fork']
- 114.3 s: None
- 1102.5 s: [4, 'The factory yard']

