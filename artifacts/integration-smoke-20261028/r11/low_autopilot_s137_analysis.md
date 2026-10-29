### artifacts/runtime/session-20261028-164420.jsonl

Session log `artifacts/runtime/session-20261028-164420.jsonl`: 900 one-second lines, 847 with the Canyon clock running.

Internal grid [480, 272], surface [5120, 2880]. 28248 frames over 847 s.

| metric | value |
|---|---|
| mean frame time (exact) | 30.86 ms (32.4 fps) |
| median second fps / 5th-percentile second fps | 39.6 / 0.7 |
| p50 of per-second p50 | 23.95 ms |
| p95 of per-second p95 | 1291.81 ms |
| p99 of per-second p99 | 1545.40 ms |
| worst single frame | 1653.9 ms |
| seconds with p99 > 33.3 ms | 587 |
| seconds with p50 > 16.7 ms | 829 |
| CPU simulation mean / p99-of-p99 | 140.23 / 1514.43 ms |
| CPU render submission mean / p99-of-p99 | 17.02 / 42.34 ms |
| remainder of the frame (acquire/present/GPU wait/UI) | -126.38 ms |

Frame time over the run (per minute of play; per-second statistics pooled):

| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 31.4 | 20.45 | 910.10 | 938.34 | 938.3 | 199.08 | 16.36 | 0.58 | 15 |
| 1 | 40.9 | 18.63 | 895.94 | 914.50 | 1564.4 | 81.67 | 16.54 | 0.58 | 54 |
| 2 | 9.5 | 1519.73 | 1648.98 | 1653.95 | 1653.9 | 900.31 | 14.42 | 0.58 | 25 |
| 3 | 29.1 | 27.15 | 1019.11 | 1549.01 | 1561.8 | 116.59 | 19.88 | 0.58 | 53 |
| 4 | 28.8 | 27.71 | 1507.44 | 1522.36 | 1522.4 | 143.23 | 18.31 | 0.58 | 50 |
| 5 | 25.6 | 26.63 | 1281.85 | 1302.33 | 1302.3 | 203.10 | 17.37 | 0.58 | 45 |
| 6 | 24.7 | 27.26 | 1294.64 | 1531.60 | 1531.6 | 301.71 | 17.23 | 0.54 | 44 |
| 7 | 37.9 | 22.60 | 244.62 | 822.19 | 1295.1 | 22.03 | 16.42 | 0.16 | 56 |
| 8 | 21.7 | 28.79 | 1343.33 | 1387.27 | 1387.3 | 355.32 | 17.57 | 0.10 | 42 |
| 9 | 21.0 | 28.07 | 1350.52 | 1356.05 | 1356.0 | 353.88 | 16.90 | 0.10 | 18 |
| 10 | 38.9 | 23.82 | 90.38 | 355.06 | 825.8 | 13.79 | 16.85 | 0.10 | 0 |
| 11 | 42.7 | 22.72 | 30.71 | 53.51 | 53.7 | 7.05 | 15.74 | 0.10 | 0 |
| 12 | 40.6 | 24.12 | 33.55 | 55.92 | 116.8 | 7.29 | 16.63 | 0.10 | 0 |
| 13 | 41.5 | 23.37 | 31.00 | 87.41 | 291.7 | 7.27 | 16.53 | 0.18 | 0 |
| 14 | 42.3 | 23.30 | 31.79 | 51.33 | 219.4 | 7.05 | 15.95 | 0.57 | 0 |
| 15 | 26.7 | 25.56 | 921.70 | 1056.97 | 1057.0 | 162.55 | 17.46 | 0.58 | 38 |
| 16 | 25.4 | 29.14 | 917.70 | 930.84 | 930.8 | 128.22 | 18.81 | 0.58 | 43 |

Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:

| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |
|---|---|---|---|---|---|---|
| 104 s | 0.6 | 1653.9 / 1653.9 / 1653.9 | 1530.28 / 1530.28 | 9.87 / 9.87 | — | enemy 129 m; 9 live AFV; rm events 0, sparks 1, parcels 249, lights 10; lights offered 10; storm 0.58; station Loader; beat 3 'The gun line at the fork' |
| 112 s | 0.7 | 1649.0 / 1649.0 / 1649.0 | 1514.99 / 1514.99 | 15.29 / 15.29 | — | enemy 129 m; 9 live AFV; rm events 2, sparks 4, parcels 256, lights 9; lights offered 9; storm 0.58; station Loader |
| 105 s | 6.7 | 111.9 / 1573.6 / 1573.6 | 95.59 / 1544.60 | 12.52 / 41.90 | 11.04 | enemy 129 m; 9 live AFV; rm events 0, sparks 1, parcels 250, lights 10; lights offered 10; storm 0.58; station Loader; beat 3 'The gun line at the fork' |
| 96 s | 0.6 | 1564.4 / 1564.4 / 1564.4 | 1534.65 / 1534.65 | 16.12 / 16.12 | — | enemy 129 m; 9 live AFV; rm events 0, sparks 0, parcels 229, lights 10; lights offered 9; storm 0.58; station Loader; beat 3 'The gun line at the fork' |
| 114 s | 0.6 | 1561.8 / 1561.8 / 1561.8 | 1532.65 / 1532.65 | 14.94 / 14.94 | — | enemy 129 m; 9 live AFV; rm events 2, sparks 1, parcels 256, lights 11; lights offered 11; storm 0.58; station Loader |

GPU passes from 462 fenced frames (pass sum: median 11.44 ms, mean 12.15 ms; fenced frame wall median 29.51 ms). Ranked by median cost per frame:

| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |
|---|---|---|---|---|---|---|---|
| 1 | `sky-visibility-static` | 2.262 | 19.8 % | 2.986 | 6.103 | 81.559 | 417/462 |
| 2 | `raster-optic-exterior` | 2.076 | 18.1 % | 1.979 | 3.182 | 5.328 | 440/462 |
| 3 | `rm-gbuffer` | 1.284 | 11.2 % | 1.299 | 2.082 | 2.794 | 462/462 |
| 4 | `rm-point-face` | 1.156 | 10.1 % | 1.109 | 2.401 | 2.867 | 462/462 |
| 5 | `rm-shadow-far` | 0.857 | 7.5 % | 0.874 | 1.035 | 1.167 | 462/462 |
| 6 | `rm-sky` | 0.566 | 4.9 % | 0.584 | 0.732 | 3.217 | 462/462 |
| 7 | `rm-sky-cover` | 0.473 | 4.1 % | 0.447 | 0.535 | 0.568 | 462/462 |
| 8 | `rm-shade` | 0.465 | 4.1 % | 0.511 | 0.762 | 0.878 | 462/462 |
| 9 | `raster-optic-current-weather` | 0.452 | 3.9 % | 0.458 | 1.134 | 1.822 | 440/462 |
| 10 | `rm-shadow-near` | 0.443 | 3.9 % | 0.499 | 0.927 | 1.014 | 462/462 |
| 11 | `sky-visibility-dynamic` | 0.350 | 3.1 % | 0.388 | 0.725 | 0.809 | 440/462 |
| 12 | `raster-optic-sky` | 0.168 | 1.5 % | 0.324 | 1.152 | 2.971 | 440/462 |
| 13 | `exposure` | 0.158 | 1.4 % | 0.188 | 0.317 | 0.341 | 462/462 |
| 14 | `rm-volume` | 0.115 | 1.0 % | 0.140 | 0.246 | 0.394 | 462/462 |
| 15 | `rm-integrate` | 0.052 | 0.5 % | 0.061 | 0.103 | 0.112 | 462/462 |
| 16 | `rm-reflect` | 0.043 | 0.4 % | 0.043 | 0.054 | 0.064 | 462/462 |
| 17 | `rm-glass` | 0.040 | 0.3 % | 0.035 | 0.049 | 0.056 | 443/462 |
| 18 | `rm-cloud-shadow` | 0.033 | 0.3 % | 0.036 | 0.070 | 0.073 | 462/462 |
| 19 | `bloom-glare` | 0.011 | 0.1 % | 0.014 | 0.024 | 0.026 | 462/462 |
| 20 | `bloom-composite` | 0.007 | 0.1 % | 0.008 | 0.013 | 0.014 | 462/462 |
| 21 | `bloom-glare-down` | 0.007 | 0.1 % | 0.008 | 0.015 | 0.019 | 462/462 |
| 22 | `bloom-glare-composite` | 0.006 | 0.1 % | 0.007 | 0.012 | 0.014 | 462/462 |
| 23 | `crush` | 0.005 | 0.0 % | 0.006 | 0.011 | 0.012 | 462/462 |
| 24 | `bloom-bright` | 0.004 | 0.0 % | 0.005 | 0.008 | 0.010 | 462/462 |
| 25 | `rm-points` | 0.004 | 0.0 % | 0.017 | 0.012 | 0.874 | 462/462 |

CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), split by the nearest enemy vehicle's range:

| seconds | canyon_mission | enemy_ai | enemy_ai.at_guns | enemy_ai.orders | enemy_ai.stealth | fx | infantry | projectiles | vehicle_contacts | wrecks_fire_haze | other | cpu_sim total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| all (847 s) | 0.01 | 0.30 | 0.10 | 0.04 | 0.05 | 5.63 | 1.24 | 0.13 | 0.03 | 0.13 | 132.76 | 140.23 |
| enemy > 420 m (364 s) | 0.01 | 0.21 | 0.10 | 0.02 | 0.02 | 4.34 | 0.97 | 0.01 | 0.02 | 0.01 | 56.34 | 61.91 |
| enemy 150-420 m (173 s) | 0.01 | 0.36 | 0.11 | 0.05 | 0.07 | 6.17 | 1.43 | 0.16 | 0.03 | 0.02 | 124.68 | 132.87 |
| enemy < 150 m (310 s) | 0.01 | 0.37 | 0.08 | 0.06 | 0.09 | 6.85 | 1.45 | 0.25 | 0.04 | 0.34 | 226.99 | 236.30 |

Raymarch work per frame (sampled once a second):

| counter | median | p95 | max |
|---|---|---|---|
| lights | 11 | 12 | 15 |
| shadowed_lights | 2 | 2 | 2 |
| parcels | 225 | 750 | 805 |
| cell_items | 642 | 61068 | 180018 |
| embers | 0 | 161 | 611 |
| spark_records | 0 | 1 | 5 |
| tracer_records | 0 | 0 | 0 |
| events | 0 | 2 | 4 |
| prepare_ms | 0.49 | 0.71 | 1.39 |
| encode_ms | 3.59 | 5.29 | 19.18 |
| point lights offered | 11 | 11 | 12 |
| live instances | 34855 | 36185 | 36701 |

Gameplay from the snapshots:

| | |
|---|---|
| first_enemy_vehicle_within_420m_s | 29.153676986694336 |
| first_main_gun_shot_s | 35.85133743286133 |
| first_hit_taken_s | 95.38520050048828 |
| main_gun_shots | 10 |
| hits_dealt | 6 |
| kills | 6 |
| hits_taken | 5 |
| penetrations_taken | 0 |
| death_cause | None |
| finished_s | None |
| final_goal_distance_m | 1997.4224853515625 |
| modules_out | [] |
| reload_s | 9 reloads; median 6.4 s, max 17.1 s |
| stalls | [] |

Mission beat changes (Canyon clock):

- 0.0 s: [1, 'Read the ground']
- 4.8 s: [2, 'Phase line Anton: the hamlet']
- 76.5 s: [3, 'The gun line at the fork']
- 111.5 s: None

