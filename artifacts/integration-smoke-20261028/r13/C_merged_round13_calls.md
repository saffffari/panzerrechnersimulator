298 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 17806 | 125.2 | 0.007 | 14.6 | 257 |
| `resolve_authoritative_terminal` | 14 | 63.4 | 4.525 | 14.4 | 257 |
| `resolve_authoritative_vehicle_impact` | 10 | 62.4 | 6.243 | 14.4 | 257 |
| `synth_damage_impact_with_pressure_delay` | 4 | 35.7 | 8.920 | 9.6 | 47 |
| `damage_structure` | 3 | 0.6 | 0.211 | 0.6 | 218 |
| `record_vehicle_scar` | 10 | 0.0 | 0.003 | 0.0 | 47 |
| `destroy_enemy_damage` | 6 | 0.0 | 0.001 | 0.0 | 296 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 257 s: advance_projectiles 14.6, resolve_authoritative_terminal 14.4, resolve_authoritative_vehicle_impact 14.4, synth_damage_impact_with_pressure_delay 9.3
- t 47 s: advance_projectiles 14.5, resolve_authoritative_terminal 14.3, resolve_authoritative_vehicle_impact 14.3, synth_damage_impact_with_pressure_delay 9.6
- t 234 s: advance_projectiles 14.4, resolve_authoritative_terminal 14.2, resolve_authoritative_vehicle_impact 14.2, synth_damage_impact_with_pressure_delay 9.4
- t 126 s: advance_projectiles 12.5, resolve_authoritative_terminal 12.2, resolve_authoritative_vehicle_impact 12.2, synth_damage_impact_with_pressure_delay 7.4
- t 10 s: advance_projectiles 2.2
