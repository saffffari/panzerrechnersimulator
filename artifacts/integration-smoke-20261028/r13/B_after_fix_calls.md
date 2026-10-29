144 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 8400 | 221.4 | 0.026 | 29.8 | 143 |
| `resolve_authoritative_terminal` | 15 | 135.0 | 8.998 | 16.1 | 155 |
| `resolve_authoritative_vehicle_impact` | 10 | 133.1 | 13.312 | 16.1 | 155 |
| `synth_damage_impact_with_pressure_delay` | 9 | 86.0 | 9.557 | 11.0 | 155 |
| `play_stereo_and_hear_at_ear` | 9 | 34.9 | 3.878 | 4.0 | 143 |
| `spawn_snow_puff` | 3 | 1.6 | 0.535 | 0.7 | 149 |
| `apply_exterior_strike` | 9 | 0.6 | 0.063 | 0.2 | 149 |
| `hear_passing_round` | 2128 | 0.9 | 0.000 | 0.2 | 149 |
| `nearest_projectile_contact` | 2128 | 25.4 | 0.012 | 0.1 | 143 |
| `present_source_ground_impact` | 15 | 0.0 | 0.003 | 0.0 | 45 |
| `finish_shell_flight_step` | 34637 | 0.9 | 0.000 | 0.0 | 95 |
| `record_vehicle_scar` | 10 | 0.0 | 0.003 | 0.0 | 47 |
| `present_source_armor_impact` | 10 | 0.1 | 0.007 | 0.0 | 143 |
| `update_projectile_muzzle_clearance` | 2128 | 0.1 | 0.000 | 0.0 | 44 |
| `record_shell_flight_segment` | 2128 | 0.2 | 0.000 | 0.0 | 137 |
| `own_armour_strike_ear_level` | 9 | 0.0 | 0.004 | 0.0 | 47 |
| `route_tree_events` | 16800 | 1.7 | 0.000 | 0.0 | 114 |
| `observe_soundscape_incoming` | 2128 | 0.2 | 0.000 | 0.0 | 149 |
| `schedule_sound_from` | 7 | 0.0 | 0.001 | 0.0 | 149 |
| `arm_authoritative_impact_presentation` | 15 | 0.0 | 0.000 | 0.0 | 45 |
| `route_scatter_events` | 16800 | 1.4 | 0.000 | 0.0 | 10 |
| `destroy_enemy_damage` | 1 | 0.0 | 0.001 | 0.0 | 49 |
| `schedule_emission` | 7 | 0.0 | 0.000 | 0.0 | 45 |
| `observe_crew_battle_terminal` | 15 | 0.0 | 0.000 | 0.0 | 47 |
| `mark_enemy_struck` | 1 | 0.0 | 0.000 | 0.0 | 49 |
| `params_from_event` | 9 | 0.0 | 0.000 | 0.0 | 127 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 143 s: advance_projectiles 29.8, resolve_authoritative_terminal 14.8, resolve_authoritative_vehicle_impact 14.8, synth_damage_impact_with_pressure_delay 9.8, play_stereo_and_hear_at_ear 4.0
- t 138 s: advance_projectiles 29.7, resolve_authoritative_terminal 14.8, resolve_authoritative_vehicle_impact 14.7, synth_damage_impact_with_pressure_delay 9.7, play_stereo_and_hear_at_ear 4.0
- t 155 s: advance_projectiles 16.3, resolve_authoritative_terminal 16.1, resolve_authoritative_vehicle_impact 16.1, synth_damage_impact_with_pressure_delay 11.0, play_stereo_and_hear_at_ear 3.9
- t 149 s: advance_projectiles 15.9, resolve_authoritative_terminal 15.4, resolve_authoritative_vehicle_impact 15.4, synth_damage_impact_with_pressure_delay 10.2, play_stereo_and_hear_at_ear 3.9, spawn_snow_puff 0.7
- t 47 s: advance_projectiles 14.9, resolve_authoritative_terminal 14.8, resolve_authoritative_vehicle_impact 14.8, synth_damage_impact_with_pressure_delay 9.5, play_stereo_and_hear_at_ear 3.8
