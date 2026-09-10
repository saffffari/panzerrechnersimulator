# Panzerrechnersystem German crew voice recording brief

Draft for native-speaker and historical review before recording. German text is intentionally short for
combat intelligibility; the English gloss defines gameplay intent, not subtitle wording. Record every cue
in **calm/urgent/panicked** intensities where marked, plus 2–4 natural variants. No modern NATO jargon.

| Cue ID | Role | Draft German | Intent / trigger | Takes |
|---|---|---|---|---|
| `cmd_target_tank_clock` | Commander | `Feindpanzer, {Uhrzeit}!` | New armored target by clock bearing | calm, urgent, panicked |
| `cmd_target_range` | Commander | `Entfernung {Zahl}!` | Follow-up range call | calm, urgent |
| `cmd_fire` | Commander | `Feuer!` | Fire authorization | urgent, panicked |
| `cmd_driver_left` | Commander | `Fahrer, links!` | Immediate maneuver | urgent, panicked |
| `cmd_driver_right` | Commander | `Fahrer, rechts!` | Immediate maneuver | urgent, panicked |
| `cmd_reverse` | Commander | `Zurück! Zurück!` | Emergency reverse | urgent, panicked |
| `gunner_identified` | Gunner | `Ziel erkannt!` | Target acquired | calm, urgent |
| `gunner_on_target` | Gunner | `Ziel erfasst!` | Lay is ready | calm, urgent |
| `loader_ap_loaded` | Loader | `Panzergranate geladen!` | AP round seated, breech shut | calm, urgent |
| `loader_he_loaded` | Loader | `Sprenggranate geladen!` | HE round seated, breech shut | calm, urgent |
| `loader_low_ammo` | Loader | `Munition wird knapp!` | Low ready-rack inventory | urgent |
| `crew_hit` | Any local crew | `Ich bin getroffen!` | Crew casualty, if still conscious | urgent, panicked |
| `crew_spall` | Any local crew | `Splitter!` | Nonfatal interior spall | urgent, panicked |
| `crew_fire` | First observer | `Feuer im Panzer!` | Interior fire begins | urgent, panicked |
| `driver_track` | Driver | `Kette ist ab!` | Track disabled | urgent |
| `driver_engine` | Driver | `Motor ist aus!` | Engine disabled | urgent, panicked |
| `radio_contact` | Radio operator | `Funkverbindung steht.` | Net restored/mission start | calm |
| `radio_lost` | Radio operator | `Funk ist ausgefallen!` | Radio module disabled | urgent |
| `ack` | Any addressed role | `Verstanden!` | Command acknowledgement | calm, urgent |

Additional wordless performances: braced impact grunts; breath knocked out; short pain cries; severe
injury cries; coughing in smoke; exertion during reload/repair; restrained fear breaths. Do not use a
long theatrical death scream for every casualty.

Recording delivery: isolated mono, 48 kHz/24-bit WAV, no reverb, compression or radio filter, consistent
mouth distance, slate containing cue ID/role/intensity/variant. Capture 30–60 seconds of room tone and
separate breaths. The game will apply compartment coloration, engine masking, positional seat cues and
radio filtering at runtime.
