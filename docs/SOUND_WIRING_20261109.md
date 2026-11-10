# Generated sounds wired to the game (9 November 2026)

The takes from `docs/SOUND_PROMPTS_20261109.md` live in the private sound store
(`assets/sounds/<ID>_<nn>.ogg` dry, `<ID>_w<nn>.ogg` heard inside the turret; never committed).
`src/sfx_rows.rs` is the one table: row ID, the game event, a 1 m reference level (estimate) and
how the row reaches the game. Nobody has auditioned the takes, so:

- **default**: the game had no sound for the event; the take plays whenever it is installed.
- **switched**: a licensed recording or a synthesised voice already plays; it stays the default and
  the take replaces it only with `KESSEL_SFX_GENERATED=1`. No existing file is touched.

Rows that replace an existing voice play at that voice's own calibrated level; the 1 m levels of
the default rows are estimates (docs/PHYSICAL_TRUTH_AUDIT.md, "Generated sound rows").

| Row | Event | Default or switched | Where |
|---|---|---|---|
| HT-06 | a round stopped by armour (world and own hull; the `_w` take inside) | switched | `game_hit_sounds::generated_hit_take` |
| HT-09 | a penetration (world and own hull) | switched | same |
| HT-01, 04, 05, 07, 08, 11, 12, 16 | the impact's row by calibre band and outcome (`sfx_rows::generated_hit_id`): under 10 mm rifle, under 20 mm AT rifle, HE on the plate under / from 100 mm, ricochet, shatter, a mantlet, held under 60 mm; the impact is threaded from every world and own-hull hit with one | switched | same |
| HT-05 | light-calibre hit recording family | switched | `audio_recorded_fx` family files |
| IN-35 | switch recording family | switched | same |
| IN-11 / IN-12 / IN-15 / IN-16 | round rammed, breech locks, case out, case on the floor | switched | owner-recording keys `shell_ram`, `breech_lock`, `case_extract`, `shell_floor` |
| IN-22 / IN-23 | hatch closed / opened | switched | keys `hatch_close`, `hatch_open` |
| DR-03 / DR-19 | starter / shutdown | switched | keys `engine_start`, `engine_shutdown` |
| TS-08 | a T-34's gun near | switched | key `t34_gun` |
| DS-01 / DS-02 | a gun / a strike far off | switched | keys `tank_gun_distant`, `impact_distant` |
| BR-08 | the engine seizes | switched | `Game::cue_engine_death` |
| BR-24 | the Tiger's propellant catching (near): a layer under the first bang | switched | `game_cookoff::cookoff_first_bang` |
| DS-11 | the first bang heard from 700 m and more: the bang's voice at its blast emission | switched | same |
| BR-25 / DS-10 | rounds going, near / from 700 m: one take under the pops, at most one in 8 s | switched | `game_cookoff::cookoff_pop` |
| SV-02 / SV-03 / SV-06 | a bailing T-34 crewman out on the roof, landing, running off (legs whole) | default | `enemy_crew` cues (Climbing to Falling, Falling to Escaping) |
| HT-18 | grenade burst on the deck (swarm) | switched (the burst's report plays otherwise) | `swarm_grenade_burst` |
| BR-01 | a track breaks | default | `Game::sfx_monitor` |
| FR-08 | the Tiger's fire goes out | default | `Game::sfx_monitor` |
| IN-04 | the turret stops after a traverse | default | `Game::sfx_monitor` |
| SV-01 | a Soviet crew's hatch flung open as they bail | default | `enemy_crew` cue |
| HT-21, FR-01, FR-03, FR-07 | petrol bottle, deck fire, engine bay catching, fixed extinguisher (swarm) | default | `swarm_runtime` |
| NF-01, 03, 04, 10-15, 17, 18, 21, 24, 25, 27 | the swarm's actions | default | `swarm_runtime` |
| NF-23, NF-26 | shots into a hatch, MG on men | not played (the rifle and MG reports already sound) | counted only |

## Loops (`looping` rows; `sfx_rows::LOOP_BANKS`, `Audio::set_generated_loop`)

A loop bank sums its takes (each made seamless, the first setting the length; mix gains are
estimates) into one loop and voices it in a positioned pool (`audio_vehicles.rs`), the same
voice as the vehicles' and fires' loops: panned, low-passed, faded in and out, its level set from
the loop's own A-weighted level so the ear level is the game's. The first request decodes the bank
on a worker thread; until it is ready, or with no take installed, the existing voice plays.

| Bank | Rows | Default or switched | Source and level |
|---|---|---|---|
| t34_idle / t34_moving | TS-01; TS-02 with TS-05 (-2 dB) | switched (the synthesised T-34 loop) | every enemy vehicle at its calibrated ear level; idle under 10 % of top speed (4 % to leave), crossfading; rev shift 0.9..1.15 |
| far_engines | DS-13 | switched | enemy vehicles beyond 800 m leave the T-34 pool and are one drone: power-summed ear levels, loudest one's bearing |
| deck_fire / deck_fire_inside | FR-01 with FR-04 (-8 dB); FR-02 with FR-05 (-6 dB) | switched (the synthesised fire) | the Tiger's burning surfaces at their ear level; the inside bank (listener buttoned or hatch open) skips the plate direction model |
| wreck_fire | FR-03 with FR-06 (-4 dB) and FR-05 (-8 dB) | switched | burning wrecks, allies' and campaign hulks |
| horizon | DS-12 | default | battle maps (any enemies), player alive: 48 dB(A) in the open [E] from the north-east, less the listener's enclosure and headset |
| radio_idle | IN-32 | default | aboard (headset), alive, radio working: 42 dB(A) in the headset [E], diotic |

With the switch on, the one-shot FR-01 / FR-03 at the swarm's fire start is not played (the fire's
loop carries it).

Rows left unwired, and why:
- **Loops not yet voiced** (IN-03 traverse, DS-15 aircraft, SV-11, NF-02, NF-16, NF-30, TS-06,
  IN-36, IN-38, TL rows marked loop): no source in the game drives them yet.
- **No such event yet**: DR-06 engine catches and DR-07 refuses (the starter sequence has no
  separate catch or refusal moment), IN-09/10/13/14 (drawn from the rack, in the arms, firing inside,
  recoil: the existing gun and loader voices cover them without separate moments wired here),
  SV-04/05, SV-07..11 (rolling, crawling and the voices: no such state or a spoken voice),
  DS-03..09, DS-14, DS-16..18, HT-02/03 (bursts: `bullet_strike_audio` plays HT-02 per burst
  already), HT-10 spall shower and HT-26 hull tail (layers, not wired), HT-13/14/15/17 (the impact
  names its plate, not a track, bin, barrel or vision block; the component layers voice those),
  HT-19, HT-20, HT-22..25, TG, TL, the rest of DR, IN, BR, TS, NF and FR.
