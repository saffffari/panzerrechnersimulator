# Adaptive sound director and optional room rack

These are two pure modules. They do not start playback, inspect the world, load assets, read a wall clock, change gun propagation, or decide whether a location is historically appropriate. Runtime integration supplies gameplay facts, an explicit seed, location tags, valid sound IDs and authored rack settings.

## Director contract

`soundscape_director::Director::new(seed)` owns deterministic SplitMix64 state. `update(dt, &Context)` returns a `Plan` with mode, night flag/identity, location and intensity. Modes serialize as `explore`, `driving`, `threat`, `combat` and `aftermath`.

- Speed is smoothed with a 1.2-second time constant. Driving enters at 1.8 m/s and leaves below 0.65 m/s; reversing uses absolute speed.
- Threat rises with a 0.4-second time constant and falls with 1.5 seconds. Threat enters at 0.45 and leaves below 0.25. It takes precedence over driving.
- Ordinary mode changes have a minimum three-second hold. An actual combat indication overrides that hold immediately.
- Combat remains active for 12 quiet seconds after the last combat indication, then enters 15 seconds of aftermath. Renewed combat interrupts either interval immediately. A long dt consumes elapsed release/aftermath time rather than stretching those phases over future updates.
- Intensity rises with a 0.4-second and falls with a 2.5-second time constant. It remains in [0, 1]; it is a mix instruction, not physical sound pressure.
- The first nonempty explicit location is adopted immediately. Subsequent changes must remain stable for two seconds; reverting to the current location cancels a pending change. `coffee_shop`, `playground`, `airport` and arbitrary authored tags are preserved without inference or special-case sound selection.
- Night and caller-owned `night_id` change immediately on a running update. **The runtime must cache the chosen night sound for the entire night and call `choose` for it only when starting a new night.** The director never chooses tracks implicitly in `update`.
- Zero, negative and nonfinite dt strictly freeze the entire director, including context changes. Resume with positive dt to apply the new context. A separate explicit `choose` call still consumes a selection, so paused runtime code should not call it.

`choose(pool, ids)` uses independent bags per pool and one explicit seeded RNG. IDs are sorted and deduplicated, making catalog/HashMap iteration order irrelevant. Each unique ID is used once per bag cycle; with at least two choices, a cycle boundary cannot repeat the last choice. A catalog membership change restarts that pool's bag while retaining its last choice to avoid immediate repetition. Empty pools return `None` without changing state. Singleton pools necessarily repeat. ID selection is not weighted in this layer.

`snapshot()` stores mode, filters, latches, timers, intensity, current/pending location, recent combat, RNG state and every bag. `restore()` accepts missing optional fields with defaults, ignores unknown optional keys, and rejects malformed types/ranges or inconsistent bags/phase timers. The required seed remains explicit. Readable f64 fields are accompanied by exact bit values so JSON round trips preserve continuation even when the serde_json parser lacks its optional `float_roundtrip` feature. Older snapshots without these bit fields remain readable. Unknown schema versions are rejected.

The classification is based on supplied samples; the director does not reconstruct events that occurred between them. Exact deterministic continuation means identical seed, call order, input contexts and dt sequence, not an assertion that differently sampled gameplay histories select the same music.

## Optional effects rack

`audio_soundscape_fx` processes stereo f32 frames at **44,100 Hz**. Use `RackConfig::default()` for dry playback, `RackConfig::preset(name)` (also available as the module-level `preset`) for an explicit starting point, then construct one `Rack` per soundscape voice. The six public controls are wet mix, nominal decay seconds, predelay milliseconds, damping cutoff, low-pass cutoff and high-pass cutoff.

| Preset | Wet | Nominal decay | Predelay | Damping | Low-pass | High-pass |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| dry / unknown | 0 | 0.8 s | 0 ms | 6,000 Hz | 22,050 Hz bypass | 0 Hz bypass |
| open_air | 0.05 | 0.22 s | 0 ms | 6,000 Hz | 18,000 Hz | 30 Hz |
| small_room | 0.18 | 0.65 s | 7 ms | 4,800 Hz | 18,000 Hz | 50 Hz |
| large_room | 0.30 | 1.8 s | 18 ms | 3,600 Hz | 16,000 Hz | 35 Hz |
| tank_cabin | 0.18 | 0.28 s | 1.5 ms | 2,600 Hz | 12,000 Hz | 55 Hz |

These are **authored algorithmic approximations**, not measured impulse responses, acoustic ray tracing, or a substitute for existing source/propagation handling. Defaults are dry and unknown preset names stay dry. Root integration chooses when raw scene recordings need treatment; this module does not reinterpret an already reverberant recording automatically.

The wet path has three early-reflection taps plus four damped feedback delay loops per channel with different stereo delays. Feedback is strictly below one and each loop uses normalized injection, avoiding unbounded constant-signal buildup. Buffers are allocated only at construction. `process` and `set_occlusion` allocate nothing and do no I/O. Configuration is immutable after construction; only occlusion changes at runtime.

`set_occlusion(amount)` interpolates the low-pass cutoff logarithmically toward at most 800 Hz and smooths the coefficient over about 30 ms. **It applies no gain attenuation:** the existing runtime applies distance/obstruction gain once. Its control smoother uses f64 state so returning to the default open path reaches exact dry bypass instead of stalling at a f32 rounding residue.

Finite samples on the default unoccluded dry path are bit-exact, including signed zero and master-bus headroom. Nonfinite inputs become zero. The active effect path guards pathological input magnitudes at ±8 and output at ±16; these are numerical safety guards, not a loudness limiter or SPL calibration. The master bus still owns ordinary reproduction/limiting. For a non-looping clip, continue feeding silence while its intended reverb tail plays; destroying the rack with the last source sample would cut that tail off.

## Validation status

The new files were parsed/formatted with rustfmt. **No Cargo build, compiled test run, GPU run or audio audition was performed by this implementation sub-agent.** Root owns integration and coordinated validation.

The director includes 13 unit tests for seeded determinism, catalog-order independence, seed variation, complete bags and cycle-boundary repeat prevention, changed/empty/singleton catalogs, combat grace/aftermath/retriggering, large-dt phase consumption, speed/threat hysteresis, explicit location/night handling, strict pause, exact JSON continuation, old optional fields, corrupted snapshots and nonfinite inputs.

The rack includes seven tests for dry bit equality, stable silence, finite bounded impulse decay/stereo output, predelay timing, smooth high-frequency occlusion with unity DC gain, malformed controls/input, and unchanged delay-buffer pointers/capacities during processing. These are implementation tests pending execution; they do not establish that the room presets sound right. Audible review with the actual source recordings remains necessary.

## Nonliteral foreground score programme

`soundscape_program::ScoreProgram` is a separate pure, seeded layer over the factual director. It returns `Choice { role: Option<String>, token: u64, gain: f32 }`. A rest is `None` with zero gain. Its RNG is independent of the director and night-bed shuffle bags. It does not invent physical sound effects, conceal factual gun/engine/impact sounds, or alter the gameplay classification.

The intention is that music conveys atmosphere without becoming a reliable proximity or state detector. The following are authored draw weights, not a claim that any individual short playthrough reaches those exact proportions:

| Factual mode | music | explore | driving | threat texture | combat | aftermath | quiet | sustain existing texture |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Explore | 25% | 35% | 8% | 12% | — | — | 20% | — |
| Driving | 25% | 17% | 30% | 10% | — | — | 18% | — |
| Threat | 10% | 50% | 10% | 10% | — | — | 20% | — |
| Combat | — | — | — | — | 65% | — | 15% | 20% |
| Aftermath | 15% | 15% | — | 30% | — | 15% | 25% | — |

The `music` role is the newly requested melodic collection. The runtime may fall back to `explore` when that pool is absent. A single foreground score lane alternates melodic and texture cues; it does not pile unrelated melodies or rhythms together. Independent cabin and night ambience may continue. Combat's sustain outcome retains an existing nonmelodic texture or existing silence; if the preceding role was melodic `music`, that outcome becomes quiet rather than introducing melodic combat music by default.

Mode-entry reactions wait a seeded 3–14 seconds for ordinary modes and 0.5–3 seconds for combat. Ordinary changes also respect a minimum six-second choice hold; rapid input churn coalesces instead of repeatedly restarting the foreground. Normal playing phases last 20–55 seconds and programmed rests 6–18 seconds. On an unchanged mode, later draws still advance the token and may choose another cue or silence. A sustain preserves the token and cue, while its level continues a smooth phrase. Non-silent gains stay within 0.5–0.9; this is programme-level headroom, with no signal-following automatic gain control.

Zero, negative or nonfinite dt freezes pending reactions, time, RNG and choice. Only one audible choice is emitted per update: missed score slots during a stalled update are coalesced rather than emitted as a catch-up burst. The optional snapshot is diagnostic and does not claim audio-frame/rack restoration.

Runtime integration should key selection on the choice token and role, apply its gain, and treat `None` as a real score rest. Deterministic factual `{mode}_enter` musical stingers would undermine the variance goal; keep physical gameplay cues factual and route optional score accents through the programme or explicit authored rules instead.

Seven additional tests cover a 200-seed role-overlap/quiet sample, identical-input replay, different-seed variation without changing the factual mode, repeated draws and dwell bounds under a stable mode, 200-seed mode churn, bounded reaction delays/no melodic combat selection, and pause behavior. They are written but were not compiled or run by this sub-agent. An independent Python check of the same initial RNG draws across seeds 0–199 produced: Explore 50 music / 68 explore / 18 driving / 23 threat / 41 quiet; Threat counts were measured before its later calm-bias adjustment; initial Combat 130 combat / 70 quiet, including sustain of its initial silence. This checks statistical intent only; integration and audible acceptance remain separate.

### Confirmed incoming-fire gate (2026-10-02)

Live Game no longer derives score combat from global/player gun counters, shots originating within 600 metres, or distance to hidden enemies. It observes collision-truncated authoritative projectile travel. Only a hostile Soviet round that contacts the player hull/body or approaches within a small local radius with clear cover/terrain/tree LOS creates the contact pulse. Bullet radii are 3 metres around the on-foot target and 6 metres around the occupied tank; cannon rounds use 12 metres. These are authored musical contact thresholds, not changes to physical hitboxes, acoustics, or damage. Every round can report contact once. A nearby gun firing away cannot trigger this gate. The factual director then supplies its existing 12-second release and aftermath timing.

Nonliteral calm/suspense choices continue independently, but only factual Combat mode can draw a new high-intensity combat role. Threat's bias is now 50% calm explore and 10% suspense (plus music/driving/rest), so suspicion does not act as an automatic musical warning. Four additional tests cover travelled-segment geometry, friendly/player exclusion (including infantry's shared projectile sentinel), 200 seeds without any combat input, and variable response after confirmed contact. These edits were parsed, not compiled or played by the sub-agent; root owns the next test build and audible validation.

### Main-gun transition and suspense phrases

`ScoreProgram::main_gun_transition(mode) -> Choice` is an explicit event API for the player's actual main-gun discharge. It immediately advances the token, cancels pending ordinary reactions, and begins a new phrase without advancing simulated time. Its role is `combat` only when the supplied factual mode is already Combat; otherwise it is `threat`. Firing the gun cannot manufacture incoming-fire knowledge. The runtime must prewarm a distinct eligible cue for that role and perform the one-time sample-clock transition on this call; `update` continues the chosen cue and merely changes its level. No persistent immediate flag or new Choice field is introduced.

Suspense phrases begin at gain 0.5 and rise smoothly to a seeded peak of 0.72–0.9 over 8–18 seconds, bounded by 45% of the current 20–55-second phrase. Combat phrases rise over 1.5–4 seconds; other material over 3–8 seconds. The last 5–10 seconds, bounded by 30% of phrase duration, relax smoothly toward 0.5 before the next selection. A queued ordinary mode transition also has a continuous lead-out from the current gain; a canceled lead-out resumes from that gain rather than stepping back to its former peak. Runtime crossfades retain responsibility for source-to-source handoffs. Tokens and roles do not change within these gain envelopes, and no signal-driven AGC is added.

Aftermath now chooses suspense on 30% of draws, with 15% each melodic music/calm explore/aftermath and 25% quiet. Calm scenes retain occasional suspense, so a rising texture does not prove hidden enemies exist. Four added tests cover immediate gunshot tokens and contact eligibility across 200 seeds, same-cue smooth build/release, smooth combat lead-out with substantial aftermath suspense, and exact seeded replay including gunshot events. Parser checks passed; root owns compiled and listening validation.


### Solar accents and damaged-engine library source

`SolarEdges` uses the actual Sun altitude. Sunrise/sunset remain separate horizon events with a ±0.25° confirmation band. The named finite **dusk** event fires on descending −6° after an observed approach at or above −5.75°; this is the end of civil twilight, not a replacement for sunset. Each event has its own once-per-day high-water mark. First loading, paused/invalid updates and obvious time jumps produce no event. Loading within the dusk arming band also cannot invent a preceding approach. Two added tests cover separate sunset/dusk sequences, exact dusk threshold, ascending exclusion, repeat suppression, loading, pause, jumps and rewinds.

The Game supplies an optional spatial `LibraryEmitter` for the new engine-hiss library lane only while its Engine module is explicitly failed and the Tiger remains alive. Healthy ignition-off, missing engine records and fuel-only failure do not qualify. Repair immediately removes eligibility. The source is the existing rear-deck engine station `(0, 1.05, −2.40)` transformed by the current hull, including slope pitch/roll. The actual camera determines distance and bearing, so the cue remains in the tank when the player walks away.

Its catalog amplitude is an **authored 1-m reference**, not a measured hiss SPL. A zero-dB reference is passed through the existing propagation model with spherical spreading and an explicitly approximate equal-octave-energy spectrum; relative gain is `10^((ear.laf_db − reference.laf_db)/20)`, with distance floored at 1 m to avoid close-source amplification. Atmosphere/ground, real world cover, one own-hull enclosure transfer and the listener headset are applied there. Cabin listeners use their normal enclosure. Exterior/head-out listeners whose ray intersects retained Tiger armor use the reciprocal closed-hull approximation. Own armor is not also added as the generic world barrier. Visual whiteout does not masquerade as an acoustic wall.

The existing octave transfer is reduced to its one-pole equivalent cutoff, then inverted into the dry rack's logarithmic 22,050→800 Hz occlusion control. This is approximate timbral shaping: it cannot reproduce the entire spectrum or cutoffs below 800 Hz. A non-dry metadata rack changes that mapping further. Runtime should apply this returned gain/pan and LP once; generic inside ambience attenuation must not be stacked on this spatial lane. There is no coolant pressure simulation, new SPL calibration, or hearing-dose claim. Root owns per-damage-episode admission, finite/loop metadata, playback fades and final audible validation. Two Game tests cover activation/repair/death and relative distance/panning/enclosure behavior; all four new tests are written and parser checked, pending root's compiled run.


### Optional musical intensity bridges

The earlier incoming-impact riser experiment is superseded. `soundscape_riser::RiserPlanner` now accepts only the programme's desired `Choice`, the actual foreground role, the existing factual Combat gate and usable source lead time. There are no enemy, projectile, hit-prediction, geographic-zone or independent fakeout APIs. “Intensity zones” here means musical tiers: explore/music/driving/aftermath are calm, threat is suspense, and combat is the higher tier. Existing ScoreProgram suspense variation remains independent.

`update(dt, current_role, &desired, combat_allowed, available_lead_s)` may bridge an upward tier change with a seeded 45% admission chance and randomized 45–95-second cooldown. Most eligible transitions use the ordinary path. Same-tier changes, decreases, silence, unknown roles, unavailable/short source material and combat without the factual gate never create a bridge. Each monotonic programme token receives at most one consideration; history is constant-size. Start a fresh planner whenever the programme resets its tokens.

The runtime must warm both the selected target and riser before permitting a bridge. `Schedule { key: desired.token, peak_in_s, lead_s }` starts a finite 1.5–4-second source slice ending at the authored peak. While `active_key()` matches the requested token, hold that target. `Complete { key }` identifies the musical switch at that peak. Source rate/pitch never changes; no physical sound is created. The runtime retains normal crossfade/sample-scheduling responsibility. Immediately after Schedule, `align_peak(key, peak_in_s)` may extend its deadline to the outgoing next-bar boundary (never earlier, finite positive and at most 16 seconds remaining). The source slice keeps its original length and pitch, starts at `aligned_peak − lead`, and the target switch uses that same aligned peak. Alignment consumes no RNG/token and leaves the cooldown unchanged. The bounded frame-clock planner is not a claim of sample-perfect transport synchronization.

A changed target, closed combat gate or changed actual foreground cancels the bridge. `interrupt()` cancels immediately for the player's main-gun transition, death or another explicit override; the main-gun cut must not wait for the riser. Cancellation preserves the cooldown and cannot reroll the same token. After `Cancel`, retire/fade the riser before normal transition handling. The planner does not itself play a blocked combat target: normal runtime selections still depend on ScoreProgram's factual gate.

Zero/invalid dt freezes all time, RNG, token and active-state changes, even if inputs change. Explicit interrupts remain available to authoritative immediate events. `snapshot()` is diagnostic and does not claim restored audio transport state. Eight new tests replace the superseded projectile tests and cover tier eligibility/combat gating, optional once-per-token admission, peak-timed completion, bounded bar alignment, cancellation, immediate override, strict pause, bounded cooldowns and deterministic replay. Source was parsed with rustfmt; root owns compiled tests and audible transition acceptance.
