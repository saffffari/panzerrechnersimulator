298 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 17862 | 101.1 | 0.006 | 14.8 | 186 |
| `resolve_authoritative_terminal` | 11 | 35.3 | 3.208 | 14.6 | 186 |
| `resolve_authoritative_vehicle_impact` | 7 | 34.8 | 4.978 | 14.6 | 186 |
| `synth_damage_impact_with_pressure_delay` | 2 | 18.6 | 9.316 | 9.4 | 168 |
| `damage_structure` | 2 | 0.1 | 0.028 | 0.0 | 287 |
| `record_vehicle_scar` | 7 | 0.0 | 0.002 | 0.0 | 45 |
| `destroy_enemy_damage` | 5 | 0.0 | 0.001 | 0.0 | 45 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 186 s: advance_projectiles 14.8, resolve_authoritative_terminal 14.6, resolve_authoritative_vehicle_impact 14.6, synth_damage_impact_with_pressure_delay 9.2
- t 168 s: advance_projectiles 14.1, resolve_authoritative_terminal 14.0, resolve_authoritative_vehicle_impact 14.0, synth_damage_impact_with_pressure_delay 9.4
- t 10 s: advance_projectiles 1.9
- t 50 s: advance_projectiles 1.7, resolve_authoritative_terminal 1.6, resolve_authoritative_vehicle_impact 1.6
- t 175 s: advance_projectiles 1.7, resolve_authoritative_terminal 1.6, resolve_authoritative_vehicle_impact 1.6
