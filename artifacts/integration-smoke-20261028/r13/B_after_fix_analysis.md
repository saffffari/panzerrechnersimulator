### artifacts/runtime/session-20261028-183543.jsonl

Session log `artifacts/runtime/session-20261028-183543.jsonl`: 144 one-second lines, 143 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 8312 frames over 143 s.

| metric | value |
|---|---|
| mean frame time (exact) | 17.25 ms (58.0 fps) |
| median second fps / 5th-percentile second fps | 59.0 / 47.5 |
| p50 of per-second p50 | 16.67 ms |
| p95 of per-second p95 | 25.97 ms |
| p99 of per-second p99 | 50.08 ms |
| worst single frame | 638.6 ms |
| seconds with p99 > 33.3 ms | 12 |
| seconds with p50 > 16.7 ms | 57 |
| CPU simulation mean / p99-of-p99 | 3.01 / 32.73 ms |
| CPU render submission mean / p99-of-p99 | 11.46 / 28.97 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | 2.79 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 58.7 | 16.64 | 21.95 | 46.74 | 638.6 | 3.30 | 10.43 | 0.58 | 25 |
| 1 | 59.6 | 16.67 | 19.97 | 38.12 | 68.9 | 2.70 | 10.82 | 0.58 | 59 |
| 2 | 52.1 | 18.54 | 34.32 | 50.08 | 53.9 | 3.05 | 15.60 | 0.58 | 24 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 1 s | 19.7 | 51.8 / 638.6 / 638.6 | 35.62 / 601.40 | 12.56 / 43.57 | — | enemy 652 m; 11 live AFV; rm events 0, sparks 0, parcels 14, lights 10; lights offered 10; storm 0.58; station Commander; beat 1 'Read the ground' |
| 140 s | 40.4 | 24.5 / 50.1 / 50.1 | 4.60 / 31.92 | 18.55 / 24.10 | 14.95 | enemy 247 m; 10 live AFV; rm events 4, sparks 0, parcels 253, lights 11; lights offered 11; storm 0.58; station Gunner; optic; beat 3 'The gun line at the fork' |
| 128 s | 49.3 | 19.9 / 48.4 / 48.4 | 3.65 / 32.73 | 15.49 / 21.04 | 15.95 | enemy 248 m; 10 live AFV; rm events 2, sparks 2, parcels 250, lights 12; lights offered 11; storm 0.58; station Gunner; optic; beat 3 'The gun line at the fork' |
| 36 s | 40.6 | 24.5 / 46.7 / 46.7 | 7.14 / 23.84 | 8.35 / 10.25 | — | enemy 375 m; 11 live AFV; 1 shells in flight; rm events 1, sparks 1, parcels 242, lights 12; lights offered 12; storm 0.58; station Loader; beat 2 'Phase line Anton: the hamlet' |
| 27 s | 54.9 | 17.9 / 45.7 / 58.8 | 3.12 / 10.40 | 10.91 / 11.51 | 11.94 | enemy 442 m; 11 live AFV; rm events 0, sparks 0, parcels 250, lights 11; lights offered 11; storm 0.58; station Commander; beat 2 'Phase line Anton: the hamlet' |

GPU passes from 72 fenced frames (pass sum: median 11.29 ms, mean 11.67 ms; fenced frame wall median 22.47 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `sky-visibility-static` | 2.208 | 19.5 % | 2.086 | 2.568 | 6.161 | 65/72 |
| 2 | `raster-optic-exterior` | 2.018 | 17.9 % | 1.947 | 4.069 | 4.120 | 70/72 |
| 3 | `rm-gbuffer` | 1.754 | 15.5 % | 1.632 | 2.168 | 2.275 | 72/72 |
| 4 | `rm-point-face` | 1.200 | 10.6 % | 1.306 | 2.600 | 2.696 | 72/72 |
| 5 | `rm-shadow-near` | 0.846 | 7.5 % | 0.719 | 1.186 | 1.200 | 72/72 |
| 6 | `rm-shadow-far` | 0.814 | 7.2 % | 0.910 | 1.239 | 1.255 | 72/72 |
| 7 | `rm-shade` | 0.640 | 5.7 % | 0.588 | 0.745 | 0.800 | 72/72 |
| 8 | `rm-sky` | 0.539 | 4.8 % | 0.582 | 0.636 | 3.086 | 72/72 |
| 9 | `rm-sky-cover` | 0.397 | 3.5 % | 0.426 | 0.503 | 0.696 | 72/72 |
| 10 | `exposure` | 0.308 | 2.7 % | 0.252 | 0.318 | 0.503 | 72/72 |
| 11 | `sky-visibility-dynamic` | 0.301 | 2.7 % | 0.301 | 0.451 | 0.501 | 70/72 |
| 12 | `raster-optic-current-weather` | 0.197 | 1.7 % | 0.224 | 0.527 | 0.541 | 70/72 |
| 13 | `rm-volume` | 0.196 | 1.7 % | 0.160 | 0.232 | 0.369 | 72/72 |
| 14 | `raster-optic-sky` | 0.170 | 1.5 % | 0.192 | 0.495 | 0.573 | 70/72 |
| 15 | `rm-integrate` | 0.097 | 0.9 % | 0.080 | 0.104 | 0.114 | 72/72 |
| 16 | `rm-cloud-shadow` | 0.067 | 0.6 % | 0.053 | 0.071 | 0.082 | 72/72 |
| 17 | `rm-glass` | 0.047 | 0.4 % | 0.039 | 0.053 | 0.055 | 71/72 |
| 18 | `rm-reflect` | 0.045 | 0.4 % | 0.046 | 0.054 | 0.066 | 72/72 |
| 19 | `bloom-glare` | 0.023 | 0.2 % | 0.018 | 0.024 | 0.025 | 72/72 |
| 20 | `bloom-glare-down` | 0.012 | 0.1 % | 0.011 | 0.016 | 0.017 | 72/72 |
| 21 | `bloom-composite` | 0.012 | 0.1 % | 0.010 | 0.013 | 0.013 | 72/72 |
| 22 | `bloom-glare-composite` | 0.011 | 0.1 % | 0.009 | 0.012 | 0.013 | 72/72 |
| 23 | `crush` | 0.010 | 0.1 % | 0.009 | 0.012 | 0.025 | 72/72 |
| 24 | `rm-points` | 0.009 | 0.1 % | 0.020 | 0.103 | 0.209 | 72/72 |
| 25 | `rm-lens` | 0.008 | 0.1 % | 0.005 | 0.010 | 0.016 | 40/72 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (143 s) | 0.01 | 0.17 | 0.02 | 0.02 | 0.05 | 0.46 | 0.72 | 0.03 | 0.01 | 0.01 | 1.61 | 3.01 |
| enemy > 420 m (35 s) | 0.01 | 0.15 | 0.02 | 0.02 | 0.03 | 0.37 | 0.71 | 0.00 | 0.01 | 0.01 | 2.36 | 3.62 |
| enemy 150-420 m (108 s) | 0.01 | 0.17 | 0.02 | 0.02 | 0.05 | 0.49 | 0.72 | 0.04 | 0.01 | 0.01 | 1.37 | 2.81 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 11 | 12 | 13 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 251 | 269 | 275 |
| cell_items | 0 | 25920 | 89319 |
| embers | 0 | 0 | 0 |
| spark_records | 0 | 1 | 2 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 4 | 4 |
| prepare_ms | 0.45 | 0.61 | 0.88 |
| encode_ms | 1.16 | 3.06 | 17.42 |
| point lights offered | 11 | 12 | 12 |
| live instances | 33874 | 34029 | 34216 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.10552215576172 |
| first_main_gun_shot_s | 34.16968536376953 |
| first_hit_taken_s | 37.198978424072266 |
| main_gun_shots | 2 |
| hits_dealt | 1 |
| kills | 1 |
| hits_taken | 9 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 2451.49267578125 |
| modules_out | ['MainGun', 'RightTrack'] |
| reload_s | 2 reloads; median 5.1 s, max 6.1 s |
| stalls | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.8 s: [2, 'Phase line Anton: the hamlet']
- 113.0 s: [3, 'The gun line at the fork']

