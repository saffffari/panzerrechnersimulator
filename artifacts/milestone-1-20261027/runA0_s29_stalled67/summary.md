### Run A0 — seed 29, pilot b0813a9 (stalled at 67.3 %: boxed by three dead T-34s)

Session log `session-20261027-194215.jsonl.gz`: 1496 one-second lines, 1495 with the Canyon clock running.

Internal grid [800, 450], surface [1280, 720]. 85706 frames over 1495 s.

| metric | value |
|---|---|
| mean frame time (exact) | 17.47 ms (57.2 fps) |
| median second fps / 5th-percentile second fps | 59.0 / 44.3 |
| p50 of per-second p50 | 16.66 ms |
| p95 of per-second p95 | 27.42 ms |
| p99 of per-second p99 | 47.63 ms |
| worst single frame | 602.7 ms |
| seconds with p99 > 33.3 ms | 88 |
| seconds with p50 > 16.7 ms | 645 |
| CPU simulation mean / p99-of-p99 | 9.06 / 37.11 ms |
| CPU render submission mean / p99-of-p99 | 5.25 / 13.87 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 3.16 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.5 | 16.69 | 20.69 | 23.96 | 602.7 | 3.00 | 5.50 | 0.58 | 13 |
| 1 | 59.8 | 16.58 | 21.01 | 30.02 | 42.7 | 8.20 | 5.08 | 0.58 | 59 |
| 2 | 53.4 | 18.14 | 28.98 | 47.88 | 57.5 | 10.98 | 6.99 | 0.58 | 60 |
| 3 | 59.2 | 16.32 | 21.39 | 75.30 | 276.8 | 10.39 | 4.95 | 0.58 | 47 |
| 4 | 56.6 | 16.29 | 27.42 | 41.54 | 41.7 | 11.42 | 5.28 | 0.49 | 60 |
| 5 | 56.4 | 16.94 | 29.83 | 39.94 | 54.2 | 9.09 | 6.64 | 0.11 | 2 |
| 6 | 59.8 | 16.68 | 21.17 | 27.62 | 33.6 | 7.13 | 5.12 | 0.10 | 0 |
| 7 | 60.0 | 16.58 | 21.35 | 26.33 | 33.2 | 6.13 | 4.67 | 0.10 | 0 |
| 8 | 60.0 | 16.60 | 20.53 | 23.29 | 32.3 | 5.99 | 5.43 | 0.10 | 0 |
| 9 | 60.0 | 16.67 | 20.79 | 24.12 | 54.6 | 5.74 | 4.99 | 0.10 | 0 |
| 10 | 60.0 | 16.61 | 20.80 | 23.56 | 61.0 | 5.55 | 5.02 | 0.28 | 0 |
| 11 | 60.0 | 16.59 | 21.78 | 27.22 | 32.3 | 6.61 | 5.08 | 0.58 | 4 |
| 12 | 58.3 | 16.73 | 23.21 | 39.07 | 48.0 | 8.49 | 4.89 | 0.58 | 16 |
| 13 | 51.7 | 18.94 | 30.28 | 51.33 | 53.2 | 12.86 | 6.05 | 0.58 | 59 |
| 14 | 51.0 | 20.22 | 27.89 | 38.33 | 46.8 | 14.26 | 4.88 | 0.58 | 60 |
| 15 | 45.7 | 20.39 | 31.33 | 40.40 | 42.2 | 16.10 | 5.44 | 0.58 | 59 |
| 16 | 55.8 | 16.70 | 25.56 | 34.79 | 49.5 | 12.64 | 4.79 | 0.58 | 30 |
| 17 | 51.6 | 18.11 | 32.39 | 43.94 | 52.1 | 13.29 | 5.83 | 0.58 | 44 |
| 18 | 54.5 | 16.88 | 28.06 | 44.38 | 47.6 | 10.92 | 5.88 | 0.58 | 60 |
| 19 | 58.9 | 16.61 | 23.87 | 28.86 | 33.9 | 8.83 | 5.03 | 0.47 | 59 |
| 20 | 58.8 | 16.61 | 24.24 | 161.93 | 165.4 | 8.60 | 4.77 | 0.11 | 60 |
| 21 | 59.2 | 16.66 | 23.28 | 36.77 | 155.4 | 8.98 | 5.16 | 0.10 | 9 |
| 22 | 60.0 | 16.66 | 20.75 | 25.43 | 29.2 | 7.39 | 4.75 | 0.10 | 0 |
| 23 | 60.0 | 16.67 | 20.63 | 23.11 | 33.4 | 6.81 | 4.44 | 0.10 | 0 |
| 24 | 59.9 | 16.62 | 21.22 | 26.09 | 33.0 | 7.21 | 4.58 | 0.10 | 0 |
| 25 | 59.8 | 16.56 | 23.17 | 24.80 | 31.5 | 8.40 | 6.01 | 0.10 | 0 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 212 s | 42.7 | 23.4 / 276.8 / 276.8 | 15.01 / 268.38 | 7.96 / 9.42 | — | enemy 61 m; 7 live AFV; rm events 3, sparks 3, parcels 631, lights 8; lights offered 6; storm 0.58; station Gunner; optic |
| 1250 s | 45.6 | 21.6 / 165.4 / 165.4 | 14.47 / 157.83 | 6.12 / 8.28 | 9.06 | enemy 13 m; 4 live AFV; rm events 2, sparks 0, parcels 5, lights 6; lights offered 6; storm 0.10; station Loader |
| 1261 s | 43.8 | 22.8 / 161.9 / 161.9 | 15.67 / 154.53 | 6.53 / 13.07 | — | enemy 13 m; 4 live AFV; rm events 6, sparks 2, parcels 35, lights 8; lights offered 7; storm 0.10; station Loader |
| 1266 s | 46.8 | 21.1 / 155.4 / 155.4 | 14.12 / 150.27 | 5.41 / 7.89 | 8.68 | enemy 13 m; 4 live AFV; rm events 4, sparks 0, parcels 5, lights 7; lights offered 7; storm 0.10; station Loader |
| 1244 s | 50.7 | 19.4 / 143.7 / 143.7 | 11.67 / 138.72 | 4.40 / 5.01 | 8.95 | enemy 13 m; 4 live AFV; rm events 2, sparks 1, parcels 5, lights 7; lights offered 7; storm 0.10; station Loader |

GPU passes from 747 fenced frames (pass sum: median 7.68 ms, mean 7.97 ms; fenced frame wall median 15.85 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-sky` | 3.396 | 44.2 % | 3.063 | 4.087 | 10.921 | 747/747 |
| 2 | `rm-gbuffer` | 1.146 | 14.9 % | 1.192 | 1.831 | 2.685 | 747/747 |
| 3 | `rm-point-face` | 0.993 | 12.9 % | 1.150 | 2.008 | 3.926 | 747/747 |
| 4 | `rm-shadow-far` | 0.900 | 11.7 % | 0.952 | 1.434 | 2.531 | 747/747 |
| 5 | `rm-sky-cover` | 0.451 | 5.9 % | 0.483 | 0.721 | 3.873 | 747/747 |
| 6 | `rm-shadow-near` | 0.436 | 5.7 % | 0.461 | 0.733 | 1.115 | 747/747 |
| 7 | `rm-shade` | 0.300 | 3.9 % | 0.311 | 0.429 | 0.564 | 747/747 |
| 8 | `rm-volume` | 0.125 | 1.6 % | 0.155 | 0.457 | 1.190 | 747/747 |
| 9 | `rm-integrate` | 0.046 | 0.6 % | 0.048 | 0.082 | 0.259 | 747/747 |
| 10 | `exposure` | 0.037 | 0.5 % | 0.040 | 0.074 | 0.251 | 747/747 |
| 11 | `bloom-glare` | 0.020 | 0.3 % | 0.021 | 0.022 | 0.042 | 747/747 |
| 12 | `rm-reflect` | 0.015 | 0.2 % | 0.016 | 0.025 | 0.037 | 747/747 |
| 13 | `bloom-composite` | 0.015 | 0.2 % | 0.016 | 0.015 | 0.030 | 747/747 |
| 14 | `bloom-glare-composite` | 0.014 | 0.2 % | 0.015 | 0.014 | 0.029 | 747/747 |
| 15 | `rm-glass` | 0.009 | 0.1 % | 0.009 | 0.011 | 0.012 | 727/747 |
| 16 | `bloom-bright` | 0.008 | 0.1 % | 0.009 | 0.009 | 0.017 | 747/747 |
| 17 | `crush` | 0.008 | 0.1 % | 0.008 | 0.014 | 0.017 | 747/747 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.010 | 0.018 | 747/747 |
| 19 | `bloom-blur-v` | 0.005 | 0.1 % | 0.005 | 0.006 | 0.011 | 747/747 |
| 20 | `bloom-blur-h` | 0.005 | 0.1 % | 0.005 | 0.006 | 0.010 | 747/747 |
| 21 | `rm-points` | 0.003 | 0.0 % | 0.005 | 0.008 | 0.211 | 747/747 |
| 22 | `rm-lens` | 0.000 | 0.0 % | 0.000 | 0.007 | 0.007 | 37/747 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (1495 s) | 0.01 | 3.16 | 0.11 | 3.00 | 0.02 | 2.12 | 0.87 | 0.02 | 0.03 | 0.01 | 2.85 | 9.06 |
| enemy > 420 m (794 s) | 0.01 | 1.16 | 0.12 | 0.99 | 0.01 | 2.13 | 0.87 | 0.00 | 0.02 | 0.01 | 2.88 | 7.09 |
| enemy 150-420 m (394 s) | 0.01 | 5.16 | 0.07 | 5.03 | 0.03 | 2.01 | 0.88 | 0.01 | 0.03 | 0.01 | 2.81 | 10.91 |
| enemy < 150 m (307 s) | 0.01 | 5.77 | 0.10 | 5.61 | 0.04 | 2.24 | 0.83 | 0.07 | 0.03 | 0.01 | 2.82 | 11.77 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 6 | 8 | 11 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 0 | 6 | 1287 |
| cell_items | 0 | 850 | 22091 |
| embers | 0 | 0 | 1365 |
| spark_records | 0 | 0 | 3 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 3 | 6 |
| prepare_ms | 0.58 | 0.87 | 1.30 |
| encode_ms | 1.16 | 1.77 | 2.53 |
| point lights offered | 6 | 7 | 8 |
| live instances | 34616 | 35388 | 36512 |

Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):

| | start | median | peak |
|---|---|---|---|
| working set | 98 MB | 2700 MB | 6915 MB |
| private bytes | 130 MB | 7615 MB | 15329 MB |
| board VRAM used | 7348 MB | 16113 MB | 20662 MB |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.54539680480957 |
| first_main_gun_shot_s | 33.57512283325195 |
| first_hit_taken_s | 212.1837615966797 |
| main_gun_shots | 27 |
| hits_dealt | 22 |
| kills | 16 |
| hits_taken | 7 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 1357.459228515625 |
| modules_out | [] |
| reload_s | 27 reloads; median 6.1 s, max 7.1 s |
| stalls | [{'from_s': 1471.2646484375, 'to_s': 1493.451416015625, 'at': [346.0439147949219, -13.558540344238281, 349.1349182128906]}] |
| tank_stopped_lines | 0 |
| autopilot_result | {'outcome': 'pilot_stalled', 'play_s': 1511.3, 'progress': 0.6726, 'reloads': 27} |
| after_action | None |
| stage_timeouts | 0 |
| panics | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.3 s: [2, 'Phase line Anton: the hamlet']
- 90.1 s: [3, 'The gun line at the fork']
- 107.3 s: None
- 687.1 s: [4, 'The factory yard']
- 765.8 s: None

