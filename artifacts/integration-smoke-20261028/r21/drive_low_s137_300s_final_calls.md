299 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 17942 | 85.1 | 0.005 | 12.8 | 183 |
| `resolve_authoritative_terminal` | 10 | 27.0 | 2.695 | 12.6 | 183 |
| `resolve_authoritative_vehicle_impact` | 9 | 18.2 | 2.025 | 12.6 | 183 |
| `synth_damage_impact_with_pressure_delay` | 1 | 9.4 | 9.358 | 9.4 | 183 |
| `update_fx` | 17942 | 9726.2 | 0.542 | 3.2 | 281 |
| `drive_casings` | 17942 | 4147.0 | 0.231 | 2.7 | 285 |
| `drive_cubic_blood` | 17942 | 2747.2 | 0.153 | 1.3 | 289 |
| `drive_interior_fire` | 17942 | 1714.2 | 0.096 | 0.7 | 195 |
| `drive_penetration_fx` | 17942 | 424.5 | 0.024 | 0.5 | 1 |
| `drive_mg_cases` | 17942 | 295.8 | 0.016 | 0.4 | 179 |
| `drive_weapon_vfx` | 17942 | 104.1 | 0.006 | 0.2 | 299 |
| `apply_authoritative_he_blast` | 1 | 0.2 | 0.209 | 0.2 | 298 |
| `drive_loose_shells` | 17942 | 91.5 | 0.005 | 0.2 | 144 |
| `drive_hot_fragment_audio` | 17942 | 8.1 | 0.000 | 0.2 | 199 |
| `drive_at_guns` | 17942 | 17.4 | 0.001 | 0.1 | 79 |
| `drive_ext_fx` | 17942 | 11.6 | 0.001 | 0.1 | 50 |
| `drive_mg_fx` | 17942 | 9.4 | 0.001 | 0.1 | 79 |
| `drive_carried_tools` | 17942 | 12.0 | 0.001 | 0.1 | 3 |
| `drive_dust_volumes` | 17942 | 5.9 | 0.000 | 0.0 | 222 |
| `drive_reload_fx` | 17942 | 17.1 | 0.001 | 0.0 | 48 |
| `damage_structure` | 1 | 0.0 | 0.037 | 0.0 | 183 |
| `drive_smoke` | 17942 | 7.3 | 0.000 | 0.0 | 20 |
| `drive_foam` | 17942 | 16.7 | 0.001 | 0.0 | 178 |
| `drive_track_repair_fx` | 17942 | 5.3 | 0.000 | 0.0 | 14 |
| `record_vehicle_scar` | 9 | 0.0 | 0.002 | 0.0 | 37 |
| `destroy_enemy_damage` | 5 | 0.0 | 0.001 | 0.0 | 37 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 183 s: advance_projectiles 12.8, resolve_authoritative_terminal 12.6, resolve_authoritative_vehicle_impact 12.6, synth_damage_impact_with_pressure_delay 9.4, update_fx 0.6
- t 298 s: advance_projectiles 8.8, resolve_authoritative_terminal 8.6, update_fx 1.7, drive_casings 1.1
- t 1 s: advance_projectiles 2.3, update_fx 0.8
- t 37 s: advance_projectiles 1.8, resolve_authoritative_terminal 1.5, resolve_authoritative_vehicle_impact 1.5
- t 41 s: advance_projectiles 1.6, resolve_authoritative_terminal 1.4, resolve_authoritative_vehicle_impact 1.4, update_fx 0.5
