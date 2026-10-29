299 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 17942 | 98.8 | 0.006 | 12.1 | 183 |
| `resolve_authoritative_terminal` | 10 | 27.4 | 2.738 | 11.9 | 183 |
| `resolve_authoritative_vehicle_impact` | 9 | 19.1 | 2.123 | 11.9 | 183 |
| `synth_damage_impact_with_pressure_delay` | 1 | 9.6 | 9.585 | 9.6 | 183 |
| `update_fx` | 17942 | 11105.4 | 0.619 | 4.1 | 291 |
| `drive_casings` | 17942 | 4677.2 | 0.261 | 3.1 | 291 |
| `drive_cubic_blood` | 17942 | 3097.1 | 0.173 | 1.3 | 19 |
| `drive_interior_fire` | 17942 | 2038.9 | 0.114 | 0.7 | 297 |
| `drive_weapon_vfx` | 17942 | 118.4 | 0.007 | 0.4 | 299 |
| `drive_mg_cases` | 17942 | 337.1 | 0.019 | 0.3 | 162 |
| `drive_penetration_fx` | 17942 | 502.6 | 0.028 | 0.2 | 10 |
| `drive_loose_shells` | 17942 | 109.8 | 0.006 | 0.1 | 166 |
| `drive_hot_fragment_audio` | 17942 | 8.1 | 0.000 | 0.1 | 107 |
| `drive_reload_fx` | 17942 | 19.8 | 0.001 | 0.1 | 233 |
| `drive_carried_tools` | 17942 | 15.6 | 0.001 | 0.1 | 145 |
| `drive_smoke` | 17942 | 8.5 | 0.000 | 0.1 | 100 |
| `drive_ext_fx` | 17942 | 15.0 | 0.001 | 0.1 | 27 |
| `drive_at_guns` | 17942 | 19.1 | 0.001 | 0.1 | 39 |
| `drive_foam` | 17942 | 20.4 | 0.001 | 0.1 | 227 |
| `drive_mg_fx` | 17942 | 9.4 | 0.001 | 0.0 | 238 |
| `apply_authoritative_he_blast` | 1 | 0.0 | 0.034 | 0.0 | 298 |
| `damage_structure` | 1 | 0.0 | 0.033 | 0.0 | 183 |
| `drive_track_repair_fx` | 17942 | 6.4 | 0.000 | 0.0 | 197 |
| `drive_dust_volumes` | 17942 | 5.1 | 0.000 | 0.0 | 204 |
| `record_vehicle_scar` | 9 | 0.0 | 0.002 | 0.0 | 183 |
| `destroy_enemy_damage` | 5 | 0.0 | 0.001 | 0.0 | 218 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 183 s: advance_projectiles 12.1, resolve_authoritative_terminal 11.9, resolve_authoritative_vehicle_impact 11.9, synth_damage_impact_with_pressure_delay 9.6, update_fx 0.5
- t 298 s: advance_projectiles 8.2, resolve_authoritative_terminal 8.1, update_fx 1.9, drive_casings 1.2, drive_interior_fire 0.5
- t 1 s: advance_projectiles 2.3
- t 161 s: advance_projectiles 2.0, resolve_authoritative_terminal 1.9, resolve_authoritative_vehicle_impact 1.8, update_fx 0.9
- t 218 s: update_fx 2.3, advance_projectiles 1.7, drive_casings 1.5, resolve_authoritative_terminal 1.3, resolve_authoritative_vehicle_impact 1.3
