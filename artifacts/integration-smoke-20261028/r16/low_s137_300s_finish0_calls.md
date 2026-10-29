298 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `update_fx` | 17943 | 13401.4 | 0.747 | 16.1 | 246 |
| `drive_casings` | 17943 | 7476.6 | 0.417 | 15.7 | 246 |
| `advance_projectiles` | 17943 | 68.4 | 0.004 | 2.5 | 10 |
| `resolve_authoritative_terminal` | 10 | 7.7 | 0.775 | 1.6 | 199 |
| `resolve_authoritative_vehicle_impact` | 6 | 6.7 | 1.120 | 1.6 | 199 |
| `drive_interior_fire` | 17943 | 1729.6 | 0.096 | 0.7 | 281 |
| `drive_cubic_blood` | 17943 | 3083.5 | 0.172 | 0.6 | 210 |
| `drive_penetration_fx` | 17943 | 375.7 | 0.021 | 0.5 | 267 |
| `drive_mg_cases` | 17943 | 367.7 | 0.020 | 0.3 | 267 |
| `drive_weapon_vfx` | 17943 | 94.8 | 0.005 | 0.2 | 143 |
| `drive_loose_shells` | 17943 | 83.2 | 0.005 | 0.1 | 275 |
| `drive_smoke` | 17943 | 8.8 | 0.000 | 0.1 | 285 |
| `drive_reload_fx` | 17943 | 15.7 | 0.001 | 0.1 | 252 |
| `drive_at_guns` | 17943 | 16.4 | 0.001 | 0.1 | 267 |
| `drive_mg_fx` | 17943 | 8.3 | 0.000 | 0.1 | 238 |
| `drive_ext_fx` | 17943 | 10.8 | 0.001 | 0.0 | 110 |
| `drive_hot_fragment_audio` | 17943 | 7.6 | 0.000 | 0.0 | 97 |
| `drive_foam` | 17943 | 17.8 | 0.001 | 0.0 | 44 |
| `drive_carried_tools` | 17943 | 13.6 | 0.001 | 0.0 | 24 |
| `drive_dust_volumes` | 17943 | 4.3 | 0.000 | 0.0 | 240 |
| `drive_track_repair_fx` | 17943 | 5.6 | 0.000 | 0.0 | 76 |
| `record_vehicle_scar` | 6 | 0.0 | 0.002 | 0.0 | 46 |
| `destroy_enemy_damage` | 6 | 0.0 | 0.001 | 0.0 | 199 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 10 s: advance_projectiles 2.5
- t 199 s: update_fx 3.7, drive_casings 3.2, advance_projectiles 1.7, resolve_authoritative_terminal 1.6, resolve_authoritative_vehicle_impact 1.6
- t 46 s: advance_projectiles 1.7, resolve_authoritative_terminal 1.5, resolve_authoritative_vehicle_impact 1.5, update_fx 0.6
- t 175 s: advance_projectiles 1.3, update_fx 0.7
- t 303 s: update_fx 3.5, drive_casings 3.0, advance_projectiles 1.1
