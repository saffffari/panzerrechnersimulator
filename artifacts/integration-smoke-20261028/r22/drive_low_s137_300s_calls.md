299 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 17942 | 80.6 | 0.004 | 11.9 | 183 |
| `resolve_authoritative_terminal` | 10 | 25.6 | 2.558 | 11.8 | 183 |
| `resolve_authoritative_vehicle_impact` | 9 | 17.8 | 1.974 | 11.7 | 183 |
| `synth_damage_impact_with_pressure_delay` | 1 | 9.0 | 9.021 | 9.0 | 183 |
| `update_fx` | 17942 | 9291.1 | 0.518 | 2.5 | 231 |
| `drive_casings` | 17942 | 3973.2 | 0.221 | 2.1 | 231 |
| `drive_interior_fire` | 17942 | 1598.2 | 0.089 | 0.9 | 63 |
| `drive_cubic_blood` | 17942 | 2625.2 | 0.146 | 0.9 | 171 |
| `drive_weapon_vfx` | 17942 | 96.6 | 0.005 | 0.2 | 299 |
| `drive_penetration_fx` | 17942 | 426.2 | 0.024 | 0.2 | 153 |
| `drive_mg_cases` | 17942 | 289.1 | 0.016 | 0.2 | 230 |
| `drive_loose_shells` | 17942 | 87.7 | 0.005 | 0.1 | 265 |
| `drive_foam` | 17942 | 16.1 | 0.001 | 0.1 | 196 |
| `drive_dust_volumes` | 17942 | 6.4 | 0.000 | 0.1 | 38 |
| `drive_carried_tools` | 17942 | 13.6 | 0.001 | 0.1 | 248 |
| `drive_reload_fx` | 17942 | 16.9 | 0.001 | 0.1 | 91 |
| `drive_mg_fx` | 17942 | 8.5 | 0.000 | 0.1 | 213 |
| `drive_track_repair_fx` | 17942 | 5.0 | 0.000 | 0.0 | 161 |
| `drive_smoke` | 17942 | 6.9 | 0.000 | 0.0 | 123 |
| `drive_ext_fx` | 17942 | 10.6 | 0.001 | 0.0 | 62 |
| `apply_authoritative_he_blast` | 1 | 0.0 | 0.026 | 0.0 | 298 |
| `damage_structure` | 1 | 0.0 | 0.021 | 0.0 | 183 |
| `drive_hot_fragment_audio` | 17942 | 7.9 | 0.000 | 0.0 | 215 |
| `drive_at_guns` | 17942 | 16.8 | 0.001 | 0.0 | 51 |
| `record_vehicle_scar` | 9 | 0.0 | 0.001 | 0.0 | 183 |
| `destroy_enemy_damage` | 5 | 0.0 | 0.001 | 0.0 | 183 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 183 s: advance_projectiles 11.9, resolve_authoritative_terminal 11.8, resolve_authoritative_vehicle_impact 11.7, synth_damage_impact_with_pressure_delay 9.0, update_fx 0.5
- t 298 s: advance_projectiles 7.8, resolve_authoritative_terminal 7.7, update_fx 1.7, drive_casings 1.2
- t 1 s: advance_projectiles 2.3
- t 41 s: advance_projectiles 1.7, resolve_authoritative_terminal 1.4, resolve_authoritative_vehicle_impact 1.4, update_fx 0.5
- t 37 s: advance_projectiles 1.6, resolve_authoritative_terminal 1.4, resolve_authoritative_vehicle_impact 1.3
