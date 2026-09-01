# Kessel Ableton sound-authoring workflow

Status: authoring architecture for Live 12 Suite. Live is not a runtime dependency.

## Goal

Diegetic sound and score share one inharmonic material language: combustion pulses, track cadence,
gear lash, hydraulic strain, armor-panel modes, fragments, fire, and damaged machinery. The score
should feel as though it grows out of the Tiger rather than arriving as a separate melodic layer.

The north star is one 57-ton machine. A patch that sounds like a keyboard instrument is a failed
machine sound even if its individual oscillators are technically plausible.

## Canonical Live project

Create a dedicated `Kessel_57t_Machine` Live Project containing:

- `Kessel_57t_Machine.als` — the canonical Set;
- `Kessel_57t_Machine.adg` — one outer Instrument Rack with 16 Macros;
- `Kessel_Telemetry.amxd` — a frozen Max for Live UDP receiver/event mapper;
- collected, owned or cleared source recordings;
- aligned export folders for dry one-shots, loops, RPM/load/perspective grids, and score stems.

Keep a Live 12.4.3 copy before allowing 12.4.5 to resave the project. Newer Sets are not guaranteed
to open in older Live releases.

## Rack chains

1. **ENGINE PRESSURE — Operator.** Mono, fixed or inharmonic combustion cadence and low pressure.
   Avoid ordinary keyboard tracking. Disable unused oscillators, voices, filter, and LFO.
2. **ENGINE MATERIAL — Granulator III.** Loop owned Maybach, exhaust, track, fan, and friction
   recordings across RPM and load. Granulation supplies material; it does not replace the sharp
   physical transient.
3. **DRIVETRAIN / PTO — Operator + Granulator III + Corpus.** Gear teeth, final-drive lash,
   hydraulic pump, traverse load, and hull coupling.
4. **IMPACT CONTACT — Operator impulse or an owned transient.** Extremely short, non-tonal
   excitation; velocity follows the modeled strike energy.
5. **ARMOR BODY — Corpus Plate/Beam + restrained Roar.** Panel-specific short modes and
   energy-dependent iron pressure. Do not let Corpus turn every plate into a bell.
6. **PENETRATION / SPALL — Granulator III Cloud + sparse independent contacts.** Drive each whizz
   and contact from fragment telemetry. No broadband white-noise wash and no rhythmic clapping.
7. **CATASTROPHIC — Operator + owned blast + Roar.** Keep pressure, steel failure, fire/fuel, and
   long tails on independent chains.
8. **SCORE RESONANCE — Meld + Granulator III Cloud.** Use the same engine/plate source corpus and
   inharmonic modes. Crossfade calm, tension, and collapse with the director state.
9. **PERSPECTIVE / SPACE.** Maintain separate interior and exterior authoring chains. Export dry
   enough that the game remains authoritative for bearing, delay, occlusion, cabin transmission,
   and environment.

## Sixteen telemetry Macros

| # | Macro | Live game source |
|---:|---|---|
| 1 | RPM | `EngineTelemetry.rpm_norm` |
| 2 | Load | `EngineTelemetry.load` |
| 3 | Speed | `EngineTelemetry.speed_norm` |
| 4 | Gear | `EngineTelemetry.gear` |
| 5 | Traverse Rate | actual turret angular speed |
| 6 | Hydraulic Load | turret hydraulic load |
| 7 | Impact Energy | modeled shell/plate event energy |
| 8 | Penetration / Residual | residual and behind-armor energy |
| 9 | Panel Size / Tune | hit panel and effective armor |
| 10 | Bearing | listener-relative event bearing |
| 11 | Station Distance | event-to-crew-seat distance |
| 12 | Interior / Exterior | hatch and listener state |
| 13 | Damage Severity | light / medium / heavy / catastrophic |
| 14 | Score Tension | director state |
| 15 | Environment / Occlusion | act, zone, and line of sight |
| 16 | Seed / Variation | deterministic gameplay seed |

Suggested event zones: held C1 engine, D1 traverse; C2–D#2 impact outcomes; E2/F2 main gun
exterior/interior; G2 breech/brass; A2/B2 destruction/cook-off. Suggested Macro Variations: Idle,
Pulling, Upshift, Ricochet, Turret Penetration, Catastrophic, Interior, Exterior, Score Calm, Score
Tension, and Score Collapse.

## Max for Live, plug-ins, and the game

Max for Live is the fastest authoring bridge. Receive game telemetry with `udpreceive`, smooth
continuous controls, and emit notes/velocity or direct device mappings inside the Rack. Use the
Live Object Model only for coarse Set, scene, and recording operations; it is incomplete and is not
the primary audio-rate control path. Initialize through `live.thisdevice`, defer Set mutations from
notifications, and freeze the `.amxd` before sharing.

There is no “Ableton SDK” that embeds Operator, Meld, Roar, Corpus, or Granulator III in the Rust
game. A raw VST3/AU is appropriate only when a compiled processor must run outside Live. RNBO is a
possible middle path for a deliberately rewritten and supported DSP core, exporting VST3/AU or C++;
native Live devices and arbitrary Max for Live patches do not automatically become RNBO exports.

The first production path is therefore:

1. drive Live/M4L from recorded or live game telemetry;
2. approve the physical sound language in the Rack;
3. export dry/aligned assets and independent stems;
4. catalogue and hash every approved export;
5. let the Rust runtime perform state, speed-of-sound delay, bearing, occlusion, cabin filtering,
   reverb selection, and final safety limiting.

## Rights boundary

Use original synthesis and owned or independently cleared recordings for isolated shipping sound
effects. Ableton permits factory content in original compositions under its terms, but that does
not make an isolated or lightly transformed factory Pack sample automatically redistributable as a
game asset. Obtain written guidance if factory material remains audible and separable.

Official references:

- [Live instrument reference](https://www.ableton.com/en/live-manual/12/live-instrument-reference/)
- [Granulator III](https://www.ableton.com/en/packs/granulator-iii/)
- [Live audio effects: Corpus and Roar](https://www.ableton.com/en/manual/live-audio-effect-reference/)
- [Instrument, Drum and Effect Racks](https://www.ableton.com/en/live-manual/12/instrument-drum-and-effect-racks/)
- [Max for Live](https://www.ableton.com/en/manual/max-for-live/)
- [Live API overview](https://docs.cycling74.com/userguide/m4l/live_api_overview/)
- [Max for Live limitations](https://docs.cycling74.com/userguide/m4l/live_limitations/)
- [RNBO audio plug-in target](https://rnbo.cycling74.com/learn/using-the-vst-audiounit-target)
- [Commercial-use rights for Live content](https://help.ableton.com/hc/en-us/articles/209768885-Commercial-Use-rights-for-Live-content)
- [Ableton EULA](https://www.ableton.com/en/eula/)
