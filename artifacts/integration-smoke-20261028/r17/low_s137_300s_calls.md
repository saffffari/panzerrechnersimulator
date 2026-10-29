288 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 16531 | 304.4 | 0.018 | 17.0 | 142 |
| `resolve_authoritative_terminal` | 24 | 148.5 | 6.188 | 16.7 | 142 |
| `resolve_authoritative_vehicle_impact` | 16 | 147.5 | 9.219 | 16.7 | 142 |
| `update_fx` | 16531 | 18541.0 | 1.122 | 14.7 | 207 |
| `drive_casings` | 16531 | 10995.3 | 0.665 | 13.8 | 207 |
| `synth_damage_impact_with_pressure_delay` | 9 | 87.5 | 9.720 | 11.5 | 142 |
| `drive_cubic_blood` | 16531 | 3175.2 | 0.192 | 1.0 | 54 |
| `drive_mg_cases` | 16531 | 1708.9 | 0.103 | 0.9 | 290 |
| `drive_penetration_fx` | 16531 | 406.9 | 0.025 | 0.6 | 271 |
| `drive_interior_fire` | 16531 | 1837.8 | 0.111 | 0.6 | 69 |
| `drive_loose_shells` | 16531 | 98.0 | 0.006 | 0.4 | 64 |
| `drive_weapon_vfx` | 16531 | 117.7 | 0.007 | 0.2 | 293 |
| `drive_carried_tools` | 16531 | 13.3 | 0.001 | 0.1 | 263 |
| `drive_smoke` | 16531 | 6.7 | 0.000 | 0.1 | 286 |
| `drive_foam` | 16531 | 21.3 | 0.001 | 0.1 | 25 |
| `drive_at_guns` | 16531 | 16.6 | 0.001 | 0.1 | 114 |
| `drive_reload_fx` | 16531 | 15.7 | 0.001 | 0.1 | 131 |
| `drive_mg_fx` | 16531 | 8.9 | 0.001 | 0.1 | 69 |
| `damage_structure` | 6 | 0.2 | 0.026 | 0.1 | 268 |
| `drive_ext_fx` | 16531 | 13.0 | 0.001 | 0.0 | 236 |
| `drive_track_repair_fx` | 16531 | 5.4 | 0.000 | 0.0 | 155 |
| `drive_hot_fragment_audio` | 16531 | 7.1 | 0.000 | 0.0 | 187 |
| `drive_dust_volumes` | 16531 | 4.6 | 0.000 | 0.0 | 73 |
| `record_vehicle_scar` | 16 | 0.0 | 0.002 | 0.0 | 46 |
| `destroy_enemy_damage` | 4 | 0.0 | 0.001 | 0.0 | 293 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 142 s: advance_projectiles 17.0, resolve_authoritative_terminal 16.7, resolve_authoritative_vehicle_impact 16.7, synth_damage_impact_with_pressure_delay 11.5, update_fx 3.8, drive_casings 3.1, drive_cubic_blood 0.5
- t 202 s: advance_projectiles 16.3, resolve_authoritative_terminal 16.2, resolve_authoritative_vehicle_impact 16.2, synth_damage_impact_with_pressure_delay 10.1, update_fx 5.6, drive_casings 4.7, drive_mg_cases 0.5
- t 291 s: advance_projectiles 15.3, resolve_authoritative_terminal 15.1, resolve_authoritative_vehicle_impact 15.1, synth_damage_impact_with_pressure_delay 9.7, update_fx 4.9, drive_casings 4.1
- t 194 s: advance_projectiles 15.1, resolve_authoritative_terminal 14.9, resolve_authoritative_vehicle_impact 14.9, synth_damage_impact_with_pressure_delay 9.4, update_fx 4.5, drive_casings 4.1
- t 177 s: advance_projectiles 15.1, resolve_authoritative_terminal 14.7, resolve_authoritative_vehicle_impact 14.7, synth_damage_impact_with_pressure_delay 9.3, update_fx 3.9, drive_casings 3.4
