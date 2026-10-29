298 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 17542 | 82.4 | 0.005 | 14.5 | 168 |
| `resolve_authoritative_terminal` | 11 | 23.3 | 2.121 | 14.2 | 168 |
| `resolve_authoritative_vehicle_impact` | 7 | 22.1 | 3.163 | 14.2 | 168 |
| `synth_damage_impact_with_pressure_delay` | 1 | 9.4 | 9.402 | 9.4 | 168 |
| `destroy_enemy_damage` | 6 | 0.1 | 0.022 | 0.1 | 220 |
| `record_vehicle_scar` | 7 | 0.0 | 0.002 | 0.0 | 47 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 168 s: advance_projectiles 14.5, resolve_authoritative_terminal 14.2, resolve_authoritative_vehicle_impact 14.2, synth_damage_impact_with_pressure_delay 9.4
- t 10 s: advance_projectiles 2.3
- t 201 s: advance_projectiles 1.8, resolve_authoritative_terminal 1.7, resolve_authoritative_vehicle_impact 1.7
- t 220 s: advance_projectiles 1.8, resolve_authoritative_terminal 1.7, resolve_authoritative_vehicle_impact 1.7
- t 51 s: advance_projectiles 1.8, resolve_authoritative_terminal 1.6, resolve_authoritative_vehicle_impact 1.6
