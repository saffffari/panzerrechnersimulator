### Run D — seed 29, final pilot da1789e (crew lost at 84.3 %, 28:56)

Session log `session-20261027-212311.jsonl.gz`: 1718 one-second lines, 1717 with the Canyon clock running.

Internal grid [800, 450], surface [1280, 720]. 97763 frames over 1717 s.

| metric | value |
|---|---|
| mean frame time (exact) | 17.60 ms (56.8 fps) |
| median second fps / 5th-percentile second fps | 59.0 / 39.7 |
| p50 of per-second p50 | 16.67 ms |
| p95 of per-second p95 | 28.90 ms |
| p99 of per-second p99 | 46.88 ms |
| worst single frame | 632.6 ms |
| seconds with p99 > 33.3 ms | 140 |
| seconds with p50 > 16.7 ms | 741 |
| CPU simulation mean / p99-of-p99 | 9.07 / 38.84 ms |
| CPU render submission mean / p99-of-p99 | 4.57 / 10.19 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 3.96 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 59.4 | 16.66 | 20.30 | 21.89 | 632.6 | 2.60 | 4.65 | 0.58 | 13 |
| 1 | 60.0 | 16.61 | 19.92 | 23.55 | 56.1 | 8.35 | 4.84 | 0.58 | 59 |
| 2 | 59.9 | 16.56 | 20.15 | 29.96 | 40.4 | 9.57 | 4.78 | 0.58 | 60 |
| 3 | 50.6 | 19.81 | 31.11 | 42.09 | 277.9 | 13.34 | 5.55 | 0.58 | 40 |
| 4 | 60.0 | 16.21 | 22.04 | 25.71 | 36.9 | 10.48 | 4.70 | 0.49 | 58 |
| 5 | 60.0 | 16.77 | 20.18 | 22.58 | 32.0 | 6.38 | 4.23 | 0.11 | 0 |
| 6 | 60.0 | 16.66 | 20.06 | 26.92 | 30.3 | 7.03 | 4.83 | 0.10 | 0 |
| 7 | 60.0 | 16.60 | 19.79 | 20.76 | 32.5 | 5.73 | 3.91 | 0.10 | 0 |
| 8 | 60.0 | 16.61 | 19.81 | 23.32 | 36.1 | 5.71 | 4.40 | 0.10 | 0 |
| 9 | 60.0 | 16.64 | 19.73 | 21.40 | 23.5 | 5.37 | 4.18 | 0.10 | 0 |
| 10 | 60.0 | 16.66 | 20.21 | 23.80 | 38.5 | 5.11 | 4.50 | 0.28 | 0 |
| 11 | 60.0 | 16.69 | 20.20 | 22.68 | 34.5 | 5.96 | 5.05 | 0.58 | 18 |
| 12 | 60.0 | 16.69 | 20.22 | 26.16 | 42.6 | 6.24 | 5.65 | 0.58 | 40 |
| 13 | 58.4 | 16.46 | 24.19 | 143.98 | 511.6 | 9.98 | 5.17 | 0.58 | 59 |
| 14 | 46.7 | 23.34 | 29.35 | 42.80 | 46.9 | 16.28 | 4.89 | 0.58 | 59 |
| 15 | 44.4 | 20.69 | 30.70 | 47.23 | 49.2 | 17.40 | 4.69 | 0.58 | 59 |
| 16 | 39.5 | 24.53 | 33.70 | 57.40 | 58.1 | 19.80 | 4.98 | 0.58 | 59 |
| 17 | 39.6 | 24.10 | 53.92 | 147.83 | 149.8 | 20.82 | 5.00 | 0.58 | 59 |
| 18 | 53.0 | 19.04 | 27.92 | 146.25 | 148.2 | 13.94 | 4.52 | 0.58 | 60 |
| 19 | 56.0 | 16.43 | 29.61 | 41.22 | 52.1 | 12.13 | 5.03 | 0.47 | 55 |
| 20 | 60.0 | 16.67 | 19.85 | 23.91 | 29.7 | 7.43 | 4.41 | 0.11 | 0 |
| 21 | 60.0 | 16.68 | 19.98 | 23.05 | 30.6 | 7.37 | 4.23 | 0.10 | 0 |
| 22 | 60.0 | 16.63 | 19.70 | 21.14 | 29.2 | 6.71 | 3.94 | 0.10 | 0 |
| 23 | 60.0 | 16.53 | 19.46 | 20.02 | 28.1 | 6.15 | 3.33 | 0.10 | 0 |
| 24 | 59.9 | 16.68 | 19.99 | 23.92 | 38.7 | 6.44 | 4.26 | 0.10 | 0 |
| 25 | 60.0 | 16.63 | 20.82 | 24.65 | 36.9 | 7.54 | 4.38 | 0.31 | 0 |
| 26 | 60.0 | 16.67 | 19.86 | 22.00 | 30.3 | 6.48 | 4.27 | 0.58 | 7 |
| 27 | 60.0 | 16.67 | 19.77 | 20.41 | 26.4 | 6.37 | 4.13 | 0.58 | 60 |
| 28 | 60.0 | 16.68 | 19.63 | 20.86 | 67.6 | 6.51 | 3.93 | 0.58 | 54 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 805 s | 31.1 | 31.8 / 511.6 / 511.6 | 23.67 / 503.89 | 7.13 / 8.16 | 15.34 | enemy 271 m; 11 live AFV; rm events 1, sparks 4, parcels 4, lights 9; lights offered 6; storm 0.58; station Gunner; optic; beat 4 'The factory yard' |
| 1077 s | 38.7 | 25.4 / 149.8 / 149.8 | 20.28 / 144.38 | 4.34 / 4.97 | 5.74 | enemy 23 m; 6 live AFV; rm events 3, sparks 0, parcels 10, lights 7; lights offered 7; storm 0.58; station Loader |
| 1082 s | 37.9 | 26.5 / 148.2 / 148.2 | 20.86 / 143.10 | 5.08 / 8.79 | — | enemy 23 m; 6 live AFV; rm events 6, sparks 3, parcels 34, lights 8; lights offered 7; storm 0.58; station Loader |
| 1071 s | 35.3 | 27.8 / 147.8 / 147.8 | 19.93 / 142.51 | 7.04 / 8.45 | 12.20 | enemy 23 m; 6 live AFV; rm events 2, sparks 4, parcels 1709, lights 9; lights offered 7; storm 0.58; station Loader |
| 1088 s | 41.5 | 24.1 / 146.3 / 146.3 | 19.06 / 141.04 | 4.52 / 5.32 | — | enemy 23 m; 6 live AFV; rm events 7, sparks 0, parcels 10, lights 6; lights offered 6; storm 0.58; station Loader |

GPU passes from 859 fenced frames (pass sum: median 7.06 ms, mean 7.36 ms; fenced frame wall median 14.37 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `rm-sky` | 2.303 | 32.6 % | 2.838 | 4.123 | 11.417 | 859/859 |
| 2 | `rm-gbuffer` | 1.124 | 15.9 % | 1.173 | 1.911 | 2.216 | 859/859 |
| 3 | `rm-shadow-far` | 0.867 | 12.3 % | 0.925 | 1.439 | 2.472 | 859/859 |
| 4 | `rm-point-face` | 0.741 | 10.5 % | 0.874 | 1.531 | 2.993 | 859/859 |
| 5 | `rm-sky-cover` | 0.401 | 5.7 % | 0.435 | 0.730 | 1.213 | 859/859 |
| 6 | `rm-shadow-near` | 0.381 | 5.4 % | 0.425 | 0.743 | 1.214 | 859/859 |
| 7 | `rm-shade` | 0.304 | 4.3 % | 0.314 | 0.415 | 0.536 | 859/859 |
| 8 | `rm-volume` | 0.127 | 1.8 % | 0.164 | 0.550 | 1.148 | 859/859 |
| 9 | `rm-integrate` | 0.046 | 0.6 % | 0.051 | 0.086 | 0.875 | 859/859 |
| 10 | `exposure` | 0.038 | 0.5 % | 0.040 | 0.074 | 0.076 | 859/859 |
| 11 | `bloom-glare` | 0.020 | 0.3 % | 0.022 | 0.041 | 0.042 | 859/859 |
| 12 | `rm-reflect` | 0.016 | 0.2 % | 0.017 | 0.024 | 0.040 | 859/859 |
| 13 | `bloom-composite` | 0.015 | 0.2 % | 0.016 | 0.029 | 0.030 | 859/859 |
| 14 | `bloom-glare-composite` | 0.014 | 0.2 % | 0.015 | 0.027 | 0.029 | 859/859 |
| 15 | `rm-glass` | 0.010 | 0.1 % | 0.009 | 0.011 | 0.014 | 850/859 |
| 16 | `bloom-bright` | 0.009 | 0.1 % | 0.009 | 0.015 | 0.017 | 859/859 |
| 17 | `crush` | 0.008 | 0.1 % | 0.008 | 0.016 | 0.017 | 859/859 |
| 18 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.013 | 0.017 | 859/859 |
| 19 | `bloom-blur-v` | 0.005 | 0.1 % | 0.005 | 0.009 | 0.011 | 859/859 |
| 20 | `bloom-blur-h` | 0.005 | 0.1 % | 0.005 | 0.009 | 0.011 | 859/859 |
| 21 | `rm-points` | 0.003 | 0.0 % | 0.006 | 0.008 | 0.214 | 859/859 |
| 22 | `rm-lens` | 0.000 | 0.0 % | 0.000 | 0.007 | 0.007 | 48/859 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (1717 s) | 0.01 | 3.71 | 0.09 | 3.56 | 0.03 | 2.00 | 0.77 | 0.03 | 0.03 | 0.01 | 2.53 | 9.07 |
| enemy > 420 m (898 s) | 0.01 | 0.98 | 0.11 | 0.84 | 0.01 | 2.02 | 0.77 | 0.00 | 0.02 | 0.01 | 2.59 | 6.40 |
| enemy 150-420 m (644 s) | 0.01 | 6.21 | 0.07 | 6.05 | 0.05 | 1.96 | 0.76 | 0.03 | 0.03 | 0.01 | 2.44 | 11.44 |
| enemy < 150 m (175 s) | 0.01 | 8.48 | 0.09 | 8.31 | 0.06 | 2.01 | 0.83 | 0.12 | 0.03 | 0.01 | 2.58 | 14.05 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 6 | 8 | 12 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 0 | 6 | 1709 |
| cell_items | 0 | 1536 | 22581 |
| embers | 0 | 0 | 2389 |
| spark_records | 0 | 1 | 4 |
| tracer_records | 0 | 0 | 4 |
| events | 0 | 3 | 8 |
| prepare_ms | 0.58 | 0.73 | 2.51 |
| encode_ms | 1.06 | 1.29 | 1.93 |
| point lights offered | 6 | 7 | 8 |
| live instances | 34610 | 36008 | 36938 |

Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):

| | start | median | peak |
|---|---|---|---|
| working set | 147 MB | 2735 MB | 7033 MB |
| private bytes | 127 MB | 7626 MB | 15407 MB |
| board VRAM used | 7386 MB | 12045 MB | 20342 MB |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.53110694885254 |
| first_main_gun_shot_s | 33.564823150634766 |
| first_hit_taken_s | 185.14820861816406 |
| main_gun_shots | 30 |
| hits_dealt | 23 |
| kills | 16 |
| hits_taken | 12 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 728.4280395507812 |
| modules_out | ['HullMg'] |
| reload_s | 29 reloads; median 6.1 s, max 7.2 s |
| stalls | [{'from_s': 616.5851440429688, 'to_s': 638.8172607421875, 'at': [-42.62503433227539, -5.75978946685791, -639.1134643554688]}, {'from_s': 1498.2369384765625, 'to_s': 1537.5972900390625, 'at': [639.631591796875, 34.04835891723633, 615.224365234375]}, {'from_s': 1582.025390625, 'to_s': 1612.2930908203125, 'at': [711.086669921875, 46.74394989013672, 615.2306518554688]}] |
| tank_stopped_lines | 0 |
| autopilot_result | {'outcome': 'knocked_out', 'play_s': 1736.4, 'progress': 0.8409, 'reloads': 29} |
| after_action | THE RUN IS OVER — CREW LOST; [("TIME", "28:56.4"), ("REACHED", "4151 m of 4926 m  (84%)"), ("BEST", "no finish yet"), ("FIRE", "30 rounds fired, 16 kills"), ("HITS TAKEN", "12 (0 through the armour)"), ("ENEMY", "19 tanks fielded against you"), ("LOST", "hull MG"), ("ATTEMPT", "seed 29  ·  start 11:48  ·  heavy snow")] |
| stage_timeouts | 0 |
| panics | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.2 s: [2, 'Phase line Anton: the hamlet']
- 90.1 s: [3, 'The gun line at the fork']
- 107.3 s: None
- 673.1 s: [4, 'The factory yard']
- 824.7 s: None
- 1552.7 s: [5, 'The final basin']

