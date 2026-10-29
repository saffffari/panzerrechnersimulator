299 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 17942 | 97.1 | 0.005 | 11.6 | 183 |
| `resolve_authoritative_terminal` | 10 | 28.6 | 2.863 | 11.5 | 183 |
| `resolve_authoritative_vehicle_impact` | 9 | 18.3 | 2.037 | 11.5 | 183 |
| `synth_damage_impact_with_pressure_delay` | 1 | 9.3 | 9.332 | 9.3 | 183 |
| `update_fx` | 17942 | 11207.7 | 0.625 | 7.8 | 50 |
| `drive_cubic_blood` | 17942 | 3269.7 | 0.182 | 7.3 | 50 |
| `drive_interior_fire` | 17942 | 2091.6 | 0.117 | 4.1 | 50 |
| `drive_casings` | 17942 | 4524.3 | 0.252 | 3.8 | 299 |
| `drive_penetration_fx` | 17942 | 517.4 | 0.029 | 0.8 | 56 |
| `drive_ext_fx` | 17942 | 16.5 | 0.001 | 0.7 | 51 |
| `drive_mg_cases` | 17942 | 325.2 | 0.018 | 0.6 | 59 |
| `drive_weapon_vfx` | 17942 | 120.1 | 0.007 | 0.5 | 60 |
| `drive_mg_fx` | 17942 | 10.6 | 0.001 | 0.3 | 58 |
| `drive_loose_shells` | 17942 | 108.4 | 0.006 | 0.2 | 54 |
| `drive_foam` | 17942 | 25.0 | 0.001 | 0.2 | 65 |
| `drive_reload_fx` | 17942 | 23.1 | 0.001 | 0.2 | 62 |
| `drive_smoke` | 17942 | 8.7 | 0.000 | 0.1 | 27 |
| `apply_authoritative_he_blast` | 1 | 0.1 | 0.105 | 0.1 | 298 |
| `drive_dust_volumes` | 17942 | 5.9 | 0.000 | 0.1 | 124 |
| `drive_at_guns` | 17942 | 22.2 | 0.001 | 0.0 | 96 |
| `damage_structure` | 1 | 0.0 | 0.036 | 0.0 | 183 |
| `drive_carried_tools` | 17942 | 14.4 | 0.001 | 0.0 | 52 |
| `drive_track_repair_fx` | 17942 | 7.0 | 0.000 | 0.0 | 145 |
| `drive_hot_fragment_audio` | 17942 | 9.1 | 0.001 | 0.0 | 120 |
| `destroy_enemy_damage` | 5 | 0.0 | 0.002 | 0.0 | 161 |
| `record_vehicle_scar` | 9 | 0.0 | 0.002 | 0.0 | 161 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 183 s: advance_projectiles 11.6, resolve_authoritative_terminal 11.5, resolve_authoritative_vehicle_impact 11.5, synth_damage_impact_with_pressure_delay 9.3, update_fx 0.6
- t 298 s: advance_projectiles 10.4, resolve_authoritative_terminal 10.1, update_fx 1.9, drive_casings 1.3
- t 1 s: advance_projectiles 2.6, update_fx 0.6
- t 142 s: advance_projectiles 2.4, update_fx 0.9
- t 161 s: advance_projectiles 2.1, resolve_authoritative_terminal 2.0, resolve_authoritative_vehicle_impact 1.9, update_fx 1.4, drive_interior_fire 1.1
