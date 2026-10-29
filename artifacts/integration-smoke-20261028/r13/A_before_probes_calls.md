415 seconds with probes; inclusive time (a probed callee also counts in its caller)

| function | calls | total ms | mean ms/call | worst call ms | at t (s) |
|---|---|---|---|---|---|
| `advance_projectiles` | 18849 | 1321.3 | 0.070 | 250.1 | 256 |
| `resolve_authoritative_terminal` | 17 | 1243.3 | 73.137 | 249.3 | 256 |
| `resolve_authoritative_vehicle_impact` | 12 | 1242.2 | 103.518 | 249.3 | 256 |
| `synth_damage_impact_with_pressure_delay` | 7 | 1198.0 | 171.146 | 242.6 | 256 |
| `play_stereo_and_hear_at_ear` | 7 | 28.3 | 4.036 | 5.1 | 256 |
| `explode_at_gun` | 1 | 1.0 | 0.988 | 1.0 | 280 |
| `infantry_pressure_blast` | 1 | 0.7 | 0.668 | 0.7 | 280 |
| `spawn_snow_puff` | 2 | 0.9 | 0.429 | 0.7 | 278 |
| `hear_passing_round` | 1798 | 0.9 | 0.000 | 0.4 | 256 |
| `apply_exterior_strike` | 7 | 0.8 | 0.115 | 0.2 | 256 |
| `infantry_contact` | 4 | 0.7 | 0.163 | 0.2 | 280 |
| `destroy_enemy_damage` | 5 | 0.1 | 0.028 | 0.1 | 49 |
| `nearest_projectile_contact` | 1798 | 21.5 | 0.012 | 0.1 | 256 |
| `damage_structure` | 4 | 0.0 | 0.009 | 0.0 | 163 |
| `present_source_ground_impact` | 18 | 0.1 | 0.003 | 0.0 | 49 |
| `route_scatter_events` | 37706 | 3.8 | 0.000 | 0.0 | 233 |
| `update_projectile_muzzle_clearance` | 1798 | 0.1 | 0.000 | 0.0 | 48 |
| `present_source_armor_impact` | 12 | 0.1 | 0.007 | 0.0 | 49 |
| `own_armour_strike_ear_level` | 7 | 0.0 | 0.004 | 0.0 | 131 |
| `record_vehicle_scar` | 12 | 0.0 | 0.002 | 0.0 | 49 |
| `observe_soundscape_incoming` | 1798 | 0.1 | 0.000 | 0.0 | 256 |
| `mark_enemy_struck` | 5 | 0.0 | 0.001 | 0.0 | 49 |
| `spawn_debris` | 5 | 0.0 | 0.001 | 0.0 | 49 |
| `arm_authoritative_impact_presentation` | 17 | 0.0 | 0.000 | 0.0 | 49 |
| `schedule_sound_from` | 21 | 0.0 | 0.001 | 0.0 | 49 |
| `route_tree_events` | 37698 | 5.0 | 0.000 | 0.0 | 12 |
| `finish_shell_flight_step` | 100978 | 2.4 | 0.000 | 0.0 | 199 |
| `spawn_blast` | 2 | 0.0 | 0.001 | 0.0 | 49 |
| `record_shell_flight_segment` | 1798 | 0.1 | 0.000 | 0.0 | 48 |
| `schedule_emission` | 35 | 0.0 | 0.000 | 0.0 | 278 |
| `observe_crew_battle_terminal` | 17 | 0.0 | 0.000 | 0.0 | 256 |
| `spawn_dust` | 10 | 0.0 | 0.000 | 0.0 | 255 |
| `params_from_event` | 7 | 0.0 | 0.000 | 0.0 | 131 |

Seconds with the slowest `advance_projectiles` call (worst single call of each probe in that second, ms):

- t 256 s: advance_projectiles 250.1, resolve_authoritative_terminal 249.3, resolve_authoritative_vehicle_impact 249.3, synth_damage_impact_with_pressure_delay 242.6, play_stereo_and_hear_at_ear 5.1
- t 269 s: advance_projectiles 200.3, resolve_authoritative_terminal 199.9, resolve_authoritative_vehicle_impact 199.8, synth_damage_impact_with_pressure_delay 194.6, play_stereo_and_hear_at_ear 4.0
- t 245 s: advance_projectiles 174.6, resolve_authoritative_terminal 174.4, resolve_authoritative_vehicle_impact 174.4, synth_damage_impact_with_pressure_delay 169.8, play_stereo_and_hear_at_ear 3.7
- t 250 s: advance_projectiles 172.2, resolve_authoritative_terminal 172.0, resolve_authoritative_vehicle_impact 172.0, synth_damage_impact_with_pressure_delay 166.8, play_stereo_and_hear_at_ear 3.7
- t 247 s: advance_projectiles 137.9, resolve_authoritative_terminal 137.8, resolve_authoritative_vehicle_impact 137.8, synth_damage_impact_with_pressure_delay 132.9, play_stereo_and_hear_at_ear 3.7
