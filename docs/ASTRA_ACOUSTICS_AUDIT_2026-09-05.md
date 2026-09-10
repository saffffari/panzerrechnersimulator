# Astra audit: impact physics and the Tiger's acoustic world

Date: 5 September 2026 UTC / 4 September on HAL. Checkout baseline: `fb89dc75fdfc3bb67812122a3be5feacaaa164c5`, with the current working changes. Line references identify the inspected implementation; function names remain the stable locator as parallel work moves lines.

## Finding

The project has a substantial, deterministic **game simulation driving procedural stereo audio**. It does not yet compute the Tiger's internal sound-pressure field from armor deformation, fracture, gas pressure and acoustic geometry. Calling the present output physically derived pressure acoustics would overstate what is implemented.

The foundation is worth retaining: live projectile flight, localized armor contacts and damage, one identified fragment population shared by visible motion and damage, contact-driven fragment audio, delayed distant reports, and listener-relative stereo DSP. The next stage needs to repair several lost connections, establish a measured or explicitly approximate source/compartment model, and let the user hear comparable in-game evidence.

This is a source and event-chain audit, with primary technical/source spot checks. It does not certify historical penetration values, injury thresholds, acoustic accuracy, or a subjectively convincing mix. No new GPU workload or listening test was run by this audit task. Both focused geometry/DSP tests passed in the centrally coordinated release build; the precise scope is recorded below.

## Implemented chain and its limits

| Stage | Evidence | What it establishes |
|---|---|---|
| Live projectile motion | `src/game.rs:20332`, `advance_projectiles`; projectile regression tests near `projectile_truth_is_identical_for_laptop_and_high_frame_schedules` | Fixed-step flight and swept collision against current targets, rather than a shot decided solely at trigger time. |
| Terminal armor result | `src/ballistics.rs:336`, `ImpactEvent`; `src/ballistics.rs:371`, `resolve` | Shell/plate data, obliquity, range-derived striking velocity, penetration estimate, terminal class and residual-effect parameters. These are empirical game laws, not material fracture simulation. |
| Local armor memory | `src/armor_damage.rs:107`, `plate_at`; `src/armor_damage.rs:125`, `record` | Scars change resistance at the struck location. Dent/hole dimensions and resistance loss are fitted rules. No acoustic aperture is generated from them. |
| Shared fragments | `src/game.rs:3199`, `generate_penetration_fragments`; `src/game.rs:22476`, `resolve_player_penetration` | The same stable-ID initial fragment bodies feed the staged visual pool and damage resolver. This prevents independently randomized damage and spark directions. |
| Fragment contacts and flight | `src/game.rs:2295`, `hot_fragment_audio_contact`; `src/game.rs:25408`, `drive_hot_fragment_audio` | Real retained fragment contact/near-pass telemetry controls short audio details in the moving hull/turret and camera frames. |
| Plate impact voice | `src/acoustics.rs:411`, `synth_impact`; `src/acoustics.rs:657`, `params_from_event` | Kinetic energy, angle, thickness, caliber, residual, panel, proximity and bearing affect a deterministic waveform. Individual hits are not peak-normalized, preserving some relative level. |
| Internal sound package | `src/acoustics.rs:907`, `damage_sound_profile`; `src/acoustics.rs:1241`, `synth_damage_impact_body` | A severity lookup combines plate impulse, authored pressure-like waveform, body effects and finite reflection taps. Live gameplay excludes the separate authored fragment shower to avoid doubling physical contact audio. |
| Stereo delivery | `src/acoustics.rs:1797`, `apply_head_shadow`; `src/audio.rs:1732`, `play_stereo` | Constant-power pan, a sub-millisecond interaural delay and far-ear filtering feed a stereo rodio sink. This is an approximate head model, not a measured HRTF. |
| Distant propagation | `src/game.rs:20080`, `schedule_sound_from`; delayed playback near `src/game.rs:14159` | Delay and attenuation are scheduled from source range; bearing/obstruction are evaluated at playback. Three room presets and simple filtering supply environment coloration. |
| Mechanical bed | `src/audio.rs:1836`, `set_engine_telemetry`; `src/audio.rs:1967`, `set_turret_traverse_hydraulic` | RPM and load affect loop pitch/gain; traverse depends on motion and engine telemetry. This is useful synchronization, but not a source-zone response matrix for engine, transmission and independent tracks. |

## Immediate defects and gaps

### 1. Local armor sound was placed at the remote shooter; repaired in this pass

Before this patch, `resolve_authoritative_vehicle_impact` used `projectile.origin - player.center()` for the hit bearing and an unrotated `CrewRole::station()` in hull coordinates for proximity. Two contacts on different nearby plates could therefore inherit nearly the same remote gun direction. Turret yaw, actual loader position and station switching did not correctly determine the listening distance.

`impact_listener_geometry` at `src/game.rs:2258` now uses the actual camera eye and forward vector with the resolved impact point. The live use is near `src/game.rs:20807`; the legacy impact presentation path uses the same helper. The delayed gun report retains its own source position.

Regression coverage:

- `impact_listener_geometry_places_opposite_armor_hits_in_opposite_ears`: opposite nearby contacts yield opposite bearings and the correct stronger channel in actual synthesized impact buffers.
- `impact_listener_geometry_follows_the_turret_rotated_listening_eye`: the real `Tank::turret_model` at three turret orientations preserves the expected contact distance and ear direction; coincident positions stay finite.

The fix improves event localization. It does not add elevation cues or update an already playing one-shot when the head turns; stereo is still baked when the sound starts.

### 2. Terminal identities are lost before audio: high priority

`params_from_event` preserves only `penetrated: bool` from `Terminal`; `Shatter` and nonpenetrating `HeBlast` lose their identities (`src/acoustics.rs:657-701`). `damage_sound_profile` classifies every nonpenetrating result as `ArmorRicochet` (`src/acoustics.rs:907-920`). The pressure layer is then excluded for that class (`src/acoustics.rs:1268`). Thus the documented external HE/shock result is acoustically treated as a ricochet on this path. The separate general HE gameplay blast call does not restore that lost typed plate-to-interior event.

There is a second mismatch for filled uncapped Soviet AP: `BR_350B` has `ShellKind::Ap` and a nonzero filler (`src/ballistics.rs:552-558`). Fragment initialization and anatomical blast policy use filler, but the audio conversion maps all `Ap` to solid `ShellClass::Ap`; the live `DamageSoundContext.internal_detonation` condition only accepts `Apcbc` or struck ammunition (`src/game.rs:22591-22597`). The same hit can produce filler-related damage and a non-explosion sound class.

Next repair: carry terminal outcome and an explicitly resolved internal burst state through the shared event. Derive damage and audio from that one state. Add cases for solid AP, filled AP, filled capped AP, external HE without breach, shatter and ammunition ignition. Do not simply infer that every filler always detonates: fuze function/delay and actual burst position are currently absent abstractions that must be named.

### 3. The live terminal solve partially recomputes and overrides projectile truth

`resolve_authoritative_terminal` passes range to `ballistics::resolve` rather than passing the live velocity (`src/game.rs:20618` vicinity). That function reconstructs speed from `ShellSpec::velocity_at(range)` (`src/ballistics.rs:378`). Cover penalties/disruption then alter a separate penetration threshold without consistently recomputing residual velocity or post-armor effect for a surviving penetration (`src/game.rs:20625-20632` vicinity).

The second `effective_penetration > event.effective_mm` check also rejects favorable stochastic penetrations below the nominal threshold, despite the deliberately symmetric marginal zone in `src/ballistics.rs:432-440`. The standalone law's tests can therefore pass while gameplay implements different odds.

Next repair: resolve one terminal event from the actual contact state, including the consumed cover budget, and test the complete gameplay boundary. Keep empirical calibration distinct from numerical consistency. This audit does not prescribe new real-weapon performance parameters.

### 4. Player-Tiger hit routing does not distinguish listening inside from standing outside

The `ProjectileVehicle::Player` branch unconditionally calls `synth_damage_impact_body` and its concussion path (`src/game.rs:20792-20890` vicinity). There is no `Mode::Foot`/exposure gate in that branch. A dismounted player still receives the internal plate/pressure package instead of the propagated external tank strike. The legacy path has a foot-mode distinction; the live authoritative path needs the same semantic decision using the correct listener.

This is especially consequential because the internal-pressure synth has bearing and severity inputs but no source-to-ear distance or hatch state (`src/acoustics.rs:1128`). Moving the listener away does not attenuate that component in the way it attenuates the direct plate impulse.

### 5. The cabin response is not yet geometry-derived

The impact response uses five fixed cross-ear taps (`src/acoustics.rs:828-868`). Their delays, gains and low-pass are identical for every crew location and armor contact. World sounds select `STEEL_INTERIOR`, `VILLAGE` or `OPEN_SNOW` based on exposure and nearby occluder count; hull transmission is a fixed one-pole filter (`src/acoustics.rs:708`) plus obstruction controls. Hatches do not change an acoustic boundary mesh, and penetration scars do not change sound leakage.

`docs/TANK_ACOUSTICS_PIPELINE.md:18-22` already proposes a low-frequency wave/high-frequency ray hybrid and baked source/listener responses. The runtime files and `Cargo.toml` contain no implementation of that convolution/IR pipeline. Treat the document as a proposed architecture, not a completed capability. Its listed station coordinates must also be refreshed from the actual rig/camera transforms before any bake.

The physically credible path is to model **source generation** and **propagation** separately: a plate excitation or airborne burst is a source; panel/air coupling, cabin response, apertures and head filtering transform that source. A room IR alone cannot make an uncalibrated impact waveform physically accurate. Linear room acoustics also does not establish a nonlinear blast-pressure history.

### 6. Fragments and pressure are detailed game approximations, not conserved fracture/blast solutions

`fragment_shower_profile` maps scalar behind-armor power, caliber, filler and residual velocity into bounded counts/sizes/speeds and cone width (`src/game.rs:3134-3193`). The pool has 768 slots. No explicit mass/energy accounting or fracture-material solve constrains the complete emitted population. Those omissions do not make the gameplay useless; they limit the claim that spall is quantitatively modeled.

The fragment collision voice receives speed, material category, distance and seed (`src/acoustics.rs:324`), but not fragment mass, contact normal/impulse or contact duration. A small and large chip at equal speed therefore share the same excitation controls. Only two collision sounds and one near-pass are permitted per penetration (`src/game.rs:6657-6662`); later contacts are intentionally curated at a cadence (`drive_hot_fragment_audio`). This is a perceptual budget, not the sum of every actual fragment's sound.

The live fragment pool already uses a fixed-step accumulator in `drive_penetration_fx` (`src/game.rs:25148-25159`), so its substep integrator's 0.05-second clamp is not a per-frame physics clock. The caller does cap accepted frame time at 0.10 seconds and backlog at 0.20 seconds. Verify long-frame policy against the rest of the game and actual audio playback at the reported performance levels before judging timing as an art issue; fixed stepping alone does not establish wall-clock agreement during severe stalls.

`crew_damage::blast_tissue_work_j` applies spherical energy fluence, average body-part area and a fixed coupling (`src/crew_damage.rs:248-256`). It does not calculate compartment pressure versus time, reflected pressure, body shielding, hatch venting or auditory dose. Its tissue thresholds are game policy. `synth_internal_pressure` independently builds finite wavelets and a chirp from a discrete severity preset (`src/acoustics.rs:1128-1219`); casualty/module counts can change that severity (`src/acoustics.rs:921-941`). Damage consequences can therefore change the nominal blast sound rather than merely adding their own sounds.

Keep source excitation independent of its downstream victims. Damage consequences may add body, mechanism and ignition sounds, but should not retroactively define the initial acoustic source.

### 7. Environmental emitter authoring stops at the schema/editor boundary

`SoundEmitter` supports 3D position, radius, volume, looping/once/reentry policy and persistence (`src/spatial_audio.rs:61-79`, `:276`). The editor loads, edits and saves this data (`src/editor.rs:754`, `:1033`, `:5064`). However, the source scan found no call to `spatial_audio::tick` or match on `EmitterAction` outside that module's own tests, and no runtime game/world/main load of `sound_emitters`.

Reproduce: `rg -n 'spatial_audio::tick|EmitterAction::' src --glob '!spatial_audio.rs'` returns no matches in this checkout. Consequently, placing a serialized sound is not yet evidence it plays during the game. The missing consumer is a fundamental environmental-storytelling task: load the authored emitter set into the live scene, route through the same listener/obstruction/state path, and show a walk-in/walk-out/reentry recording.

### 8. The missing Pizza bank was repaired during this audit; foundation coverage remains narrow

At initial inspection, `assets/sounds/pizza_private` did not exist and selected fingerprints in `catalog/pizza_audio_selection.v1.json` were null. The loader deliberately rejected pending fingerprints (`src/audio.rs:881-891`) and fell back to procedural audio (`src/audio.rs:1433` vicinity).

The parallel asset audit has since populated the reviewed fingerprints and materialized the owner bank with `BANK-RECEIPT.json`. Reinspection confirms eleven OGG cues: three pistol, four rifle, and four distant-explosion variants. This closes the missing-bank/file-provenance step. It does not establish playback on the audio device or a complete Pizza sound foundation. The mechanical-handling selections remain empty, and this bank adds neither music nor a complete machine/ambience library. The owner launcher now enables the reviewed bank by default (respecting explicit opt-out), and the parent runtime log confirms all eleven layers loaded. A heard, live-mix acceptance test remains outstanding.

The selection manifest also defines the procedural base as mandatory and protects engine, ambience, primary gun and impact identities; the permitted Pizza clips are restrained overlays. That policy predates the user's current request to make Pizza the foundational sound/art language. It should be revisited explicitly during source audition and integration. The owned bundle is sufficient authorization to inspect and prepare a local bank; preserve its actual license/provenance and do not let old acquisition wish lists drive unnecessary purchases.

### 9. Mix validation and real-time delivery need their own acceptance gate

Most transient DSP allocates and renders full buffers synchronously before appending a new sink (`src/acoustics.rs:1245`; `src/audio.rs:1732-1748`). World sounds also allocate and run their tail at release (`src/audio.rs:1183-1204`). Voices are retained until empty, with no general transient cap (`src/audio.rs:1569`). Individual soft limits are not a final output-bus limiter when multiple sounds overlap.

Current concussion control changes live voice gain; the muffle applied in `play_world_muffled` affects newly rendered world events. It is not an evolving master filter over every already playing source (`src/audio.rs:1704`, `:1713`). Finished stereo buffers do not follow subsequent head motion. The current short impact duration makes this less visible than it would be for long voices or room tails.

No source/reference pressure calibration, output-device calibration or verified acoustic-dose relationship was found. Existing tests named “fatigue safe” or “unsafe peak” constrain digital signal properties; they do not establish a safe physical listening level or a historically exact level. Product playback should remain comfortable and user-controlled; perceptual intensity must come from contrast, timing and spatial cues rather than reproducing harmful real pressure.

## What happened to the earlier research

The research is retained and substantially richer than the live audio mapping:

- `docs/research/inside-the-steel-box-report-source.md` contains a named witness/source ledger and separate confidence for embodied experience and battlefield identification (`:11-22`, `:285-326`). The sound section (`:207-211`) preserves differing impact metaphors, intercom dependence and masking. It is a valuable authoring reference, not a measured spectrum or Tiger transfer function.
- Degen's original oral-history transcript remains available. The relevant passage describes the subjective experience of enclosure, impact and armor scale. It supports the emotional/acoustic brief, not an exact frequency or gain prescription. [Degen interview, checked 5 September 2026](https://iremember.ru/en/memoirs/tankers/degen-ion-lazarevich-/)
- The report's Fischer/Pliska material is explicitly mediated through a museum-associated secondary summary, with unresolved variant/unit details (`:79`, `:293`). Do not silently upgrade it to a calibrated Tiger I recording or fully inspected original interview.
- `docs/research/04-ballistics-armor.md:384-400` mixes secondary tables, forums, encyclopedia pages and game conventions. These do not collectively certify the current parameter table as primary-source validated. Preserve a per-anchor source/angle/test-definition ledger and held-out comparisons before making that claim.
- `docs/research/data/acoustics.json` is a design-recipe collection. Its equations, broad approximations and literature suggestions are not a calibration receipt. Some assertions, including the scope of modal reconstruction quality, are stronger than the source warrants.

Two old documents now misdescribe the implementation. `docs/ACOUSTICS.md:5-8` prioritizes spectacle over accuracy, while the current user has elevated acoustic credibility. Its “struck resonator/modal frequencies” account is stale: live `synth_impact` deliberately avoids a simultaneous modal bank and uses finite wavelets (`src/acoustics.rs:435-505`). `TANK_ACOUSTICS_PIPELINE.md:6-7` describes a generic Schroeder impact tail, but live impacts now use fixed sparse returns; world effects still use Schroeder reverb. Keep this audit as the current receipt until those documents are reconciled.

## Practical next implementation order

1. **Preserve and test causality.** Complete the corrected listener/contact geometry, terminal-class/burst routing, inside/outside routing, and live contact-state terminal solve. Add trace records with source event ID, simulation time, source frame/position, listener pose, terminal outcome, burst state and selected sound profile.
2. **Deliver one repeatable audio/visual test scene.** A short Tiger interior sequence should contain comparable held and penetrated strikes, a filled-shell case, a separate distant report, then the same contact at two crew locations and open/closed exposure. Record the actual gameplay mix with a common gain and event timestamps. Include a dry-source stem and final stereo stem; do not replace live collision audio with the more elaborate offline authored shower.
3. **Make the owned art/sound foundation audible.** Audition and validate the now-materialized eleven-cue Pizza bank, expand the source selection deliberately, then connect authored 3D emitters to gameplay. Demonstrate an environment with located sound sources, obstruction and meaningful silence before adding more scenes.
4. **Introduce a small validated compartment response.** Use current rig coordinates and a simplified watertight acoustic proxy. Begin with a few source zones/listeners and two aperture states. Archive an explicitly approximate baseline and its material assumptions. Add low-frequency structural/air response and geometric high-frequency response only with benchmark and comparison evidence. CPU convolution/response interpolation should keep acoustic runtime independent of neural-network GPU priority.
5. **Expand mechanical sources and mix control.** Separate engine, front transmission, left/right tracks, suspension, breech and turret sources using actual simulation telemetry; introduce bounded voices and a shared, continuously filtered listener bus. Profile CPU time, synthesis latency, allocation count and peak overlap under the same scenarios as renderer/AI tests.

The hybrid approach already proposed is technically reasonable: COMSOL's own example combines low-frequency wave methods with high-frequency ray tracing and explicitly explains the missing modal behavior in ray-only results. This supports the method, not a preselected Tiger crossover frequency or any existing result. [COMSOL hybrid example](https://www.comsol.com/model/modeling-room-acoustics-using-hybrid-pressure-acoustics-and-ray-tracing-methods-108781)

Steam Audio exposes reflected IR baking, ray/bounce controls and Ambisonic order; it is a possible offline propagation component, not a weapon-impact or structural-fracture solver. [Steam Audio baking API](https://valvesoftware.github.io/steam-audio/doc/capi/baking.html)

Recorded-source modal fitting is another practical option for believable impact variation. The 2011 Microsoft/ACM work extracts modes and residuals from recorded impacts; its quality claim concerns that reconstructed material, not arbitrary hand-selected modal/noise values. [Lloyd, Raghuvanshi and Govindaraju, I3D 2011](https://www.microsoft.com/en-us/research/publication/sound-synthesis-impact-sounds-video-games/)

## Acceptance evidence required

| Gate | Pass evidence |
|---|---|
| Event agreement | One trace shows collision, armor outcome, fragments, burst and sound all using the same resolved event; filled AP and nonbreaching HE do not silently become the wrong audio class. |
| Coordinate agreement | Opposite plate contacts, head turn, turret rotation, walking loader, station transition and dismounted listener behave correctly; compare actual stereo energy/delay and captured poses. |
| Time agreement | The same shot/fragment scene run under fast and slow frame schedules preserves terminal and contact timing; sound-report delay is explicitly checked. |
| Acoustic model credibility | Units, geometry/material/response provenance and approximation labels are saved. Compare independent source/response benchmarks, not only tests of chosen constants. |
| Historical credibility | Each authored sensation has a witness/source/confidence record. Testimony informs experience; measured/technical data informs quantitative DSP. |
| Mix and performance | Actual full-mix recording and CPU p95/p99 under representative movement/fire/AI load; inspect peak overlap, dropped events, clipping, recovery and speech intelligibility. No claim of safe physical SPL from normalized samples. |
| Environmental storytelling | Saved editor emitters play at their real 3D locations in gameplay, obey obstruction/reentry, and combine with a visible scene. |
| User review | A short labeled video with actual stereo audio and reproducible command, common gain, event receipt and frame rate; no requirement to play through the campaign to reach the mechanic. |

## Validation receipt for this patch

The parent audit task ran the two focused `impact_listener_geometry_*` tests in the release build; both passed. They verify geometry and synthesized left/right channel energy only. The centrally coordinated release suite subsequently passed 710 tests across 20 suites with one pre-existing ignored test, after a separately discovered release texture-atlas packing bug was repaired. Neither these tests nor this report establishes an in-game listening pass, a measured waveform comparison, or pressure-field accuracy.
