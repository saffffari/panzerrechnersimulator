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
| HT-05 | light-calibre hit recording family | switched | `audio_recorded_fx` family files |
| IN-35 | switch recording family | switched | same |
| IN-11 / IN-12 / IN-15 / IN-16 | round rammed, breech locks, case out, case on the floor | switched | owner-recording keys `shell_ram`, `breech_lock`, `case_extract`, `shell_floor` |
| IN-22 / IN-23 | hatch closed / opened | switched | keys `hatch_close`, `hatch_open` |
| DR-03 / DR-19 | starter / shutdown | switched | keys `engine_start`, `engine_shutdown` |
| TS-08 | a T-34's gun near | switched | key `t34_gun` |
| DS-01 / DS-02 | a gun / a strike far off | switched | keys `tank_gun_distant`, `impact_distant` |
| BR-08 | the engine seizes | switched | `Game::cue_engine_death` |
| HT-18 | grenade burst on the deck (swarm) | switched (the burst's report plays otherwise) | `swarm_grenade_burst` |
| BR-01 | a track breaks | default | `Game::sfx_monitor` |
| FR-08 | the Tiger's fire goes out | default | `Game::sfx_monitor` |
| IN-04 | the turret stops after a traverse | default | `Game::sfx_monitor` |
| SV-01 | a Soviet crew's hatch flung open as they bail | default | `enemy_crew` cue |
| HT-21, FR-01, FR-03, FR-07 | petrol bottle, deck fire, engine bay catching, fixed extinguisher (swarm) | default | `swarm_runtime` |
| NF-01, 03, 04, 10-15, 17, 18, 21, 24, 25, 27 | the swarm's actions | default | `swarm_runtime` |
| NF-23, NF-26 | shots into a hatch, MG on men | not played (the rifle and MG reports already sound) | counted only |

Rows left unwired, and why:
- **Loops** (IN-03 traverse, TS-01/02/05 T-34 engine and tracks, DS-12/13/15, FR-01..06 as loops,
  SV-11, NF-02, NF-16, NF-30, TL rows marked loop): the game has no looping voice for a generated
  take yet; FR-01 plays once at a fire's start.
- **No such event yet**: DR-06 engine catches and DR-07 refuses (the starter sequence has no
  separate catch or refusal moment), IN-09/10/13/14 (drawn from the rack, in the arms, firing inside,
  recoil: the existing gun and loader voices cover them without separate moments wired here),
  BR-24/25, DS-10/11 (the cook-off's existing bangs; not split into burn and crackle moments),
  SV-02..11 (the bailing crew's later moments and voices), DS-03..09, DS-14, DS-16..18, HT-01..04,
  HT-07, HT-08, HT-10..17, HT-19, HT-20, HT-22..26 (the hit path carries a family, not the calibre
  or the struck part), TG, TL, the rest of DR, IN, BR, TS, NF and FR.
