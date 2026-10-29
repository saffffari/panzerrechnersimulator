298 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `update_fx` | 17917 | 22424.3 | 1.252 | 26.8 | 286 |
| `drive_casings` | 17917 | 15146.8 | 0.845 | 26.2 | 286 |
| `advance_projectiles` | 17917 | 129.0 | 0.007 | 15.3 | 124 |
| `resolve_authoritative_terminal` | 14 | 38.3 | 2.737 | 15.1 | 124 |
| `resolve_authoritative_vehicle_impact` | 7 | 36.0 | 5.138 | 15.1 | 124 |
| `synth_damage_impact_with_pressure_delay` | 2 | 18.9 | 9.434 | 9.7 | 124 |
| `drive_cubic_blood` | 17917 | 3051.8 | 0.170 | 1.7 | 107 |
| `drive_interior_fire` | 17917 | 1761.3 | 0.098 | 1.3 | 45 |
| `drive_mg_cases` | 17917 | 1685.1 | 0.094 | 1.0 | 238 |
| `apply_authoritative_he_blast` | 1 | 1.0 | 0.965 | 1.0 | 262 |
| `drive_penetration_fx` | 17917 | 392.3 | 0.022 | 0.4 | 94 |
| `drive_track_repair_fx` | 17917 | 5.6 | 0.000 | 0.2 | 300 |
| `drive_weapon_vfx` | 17917 | 106.3 | 0.006 | 0.2 | 44 |
| `drive_carried_tools` | 17917 | 12.2 | 0.001 | 0.1 | 235 |
| `drive_foam` | 17917 | 20.2 | 0.001 | 0.1 | 120 |
| `drive_loose_shells` | 17917 | 82.9 | 0.005 | 0.1 | 143 |
| `drive_mg_fx` | 17917 | 8.3 | 0.000 | 0.1 | 95 |
| `drive_smoke` | 17917 | 8.3 | 0.000 | 0.1 | 265 |
| `drive_at_guns` | 17917 | 16.2 | 0.001 | 0.1 | 237 |
| `drive_dust_volumes` | 17917 | 5.8 | 0.000 | 0.1 | 266 |
| `drive_reload_fx` | 17917 | 13.8 | 0.001 | 0.1 | 246 |
| `drive_ext_fx` | 17917 | 10.9 | 0.001 | 0.0 | 36 |
| `record_vehicle_scar` | 7 | 0.0 | 0.003 | 0.0 | 47 |
| `drive_hot_fragment_audio` | 17917 | 7.5 | 0.000 | 0.0 | 147 |
| `destroy_enemy_damage` | 5 | 0.0 | 0.001 | 0.0 | 148 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 124 s: advance_projectiles 15.3, resolve_authoritative_terminal 15.1, resolve_authoritative_vehicle_impact 15.1, synth_damage_impact_with_pressure_delay 9.7, update_fx 3.5, drive_casings 3.1
- t 47 s: advance_projectiles 14.9, resolve_authoritative_terminal 14.7, resolve_authoritative_vehicle_impact 14.7, synth_damage_impact_with_pressure_delay 9.2
- t 10 s: advance_projectiles 2.3
- t 49 s: update_fx 7.4, drive_casings 6.9, advance_projectiles 1.8, resolve_authoritative_terminal 1.6, resolve_authoritative_vehicle_impact 1.6
- t 137 s: update_fx 11.5, drive_casings 11.2, advance_projectiles 1.7, resolve_authoritative_terminal 1.5, resolve_authoritative_vehicle_impact 1.5
