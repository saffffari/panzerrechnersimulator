# Sound mass review — 29 September 2026

## Continuous engine source

The installed sound directory has no `engine_loop.ogg`, so the ordinary Tiger engine used the
procedural Maybach source. Previously its combustion pulses, hull/air filters, gear noise and
stochastic rumble were baked into one seven-second loop, then the whole waveform was resampled
at 0.78–2.42 times its source rate. Raising RPM therefore raised the sound of the hull itself and
shortened every decay. A constant physical enclosure should not shrink with engine RPM.

The procedural runtime now generates that same source continuously at the output sample rate.
The existing authored RPM mapping changes only the combustion arrival clock. All existing filter
frequencies and decay times remain in Hz and seconds. The whole voice is measured at seven rates,
then placed at the existing calculated engine level using one gain; individual components are not
normalised separately. No sub-bass tone or loudness lift was added. The engine's scalar enclosure
level, headset level, listener reflex and master reproduction transform are unchanged by this slice.
Optional recordings, if enabled and present, retain the previous loop/resampling path.

Frequency basis and limits:

- The existing source fires at an average 72 Hz before RPM scaling, corresponding to approximately
  720 rpm for a four-stroke V12 (six firings per crank revolution). See the derivation in
  `docs/research/SOUND_LEVELS_20260928.md` §7. The existing playback mapping is still an authored
  pseudo-gearbox, not a measured crankshaft or drivetrain simulation, and its upper endpoint does
  not establish the real Tiger's governed RPM.
- The inherited hull low-pass at 74 Hz and stochastic structure band between 29 and 210 Hz are
  authored spectral estimates, not measured Tiger eigenmodes. The existing report cites measured
  analogue vehicle cabin peaks at 20–63 Hz and higher unweighted than A-weighted levels; it does
  not validate these particular coefficients. This change preserves their frequencies instead of
  moving them with RPM. Existing 18 Hz DC rejection and 1420 Hz source ceiling also stay fixed.
- Continuous fan/gear noise and lash retain their previous authored coefficients and timing.
  Track travel does not yet drive separate left/right link contacts. The source is not a recording,
  structural solver, measured exhaust model, or acoustic geometry solution.
- The own-engine perspective still changes only its calculated level: inside, head-out and on-foot
  share a source timbre. On-foot own-engine propagation discards its spectral transfer/bearing;
  correct continuous transport and source positions are separate remaining work.
- Enemy engines still use the old T-34 loop; their rate and low-pass filtering are applied after a
  speed-one LAF measurement. A separate calibration correction is still needed there.

The CPU engine tests check that
combustion arrival cadence changes while identical structural histories retain their absolute
clock; whole-voice LAF matches the existing target at representative load/RPM combinations;
streaming is deterministic and finite across invalid parameter inputs and arbitrary sample
partitions; and shutting down the live capture route reaches exact digital silence. The existing
synthesis/band-limit test remains applicable to the exported legacy loop.

CPU results: `cargo test --bin panzerrechnersystem engine -- --nocapture` passed all 11 selected
checks. The arrival-clock probe measured 57.5 Hz at 0.8× and 172.5 Hz at 2.4× with identical
structural-noise/lash histories at the same elapsed time. A subsequent endpoint calibration run
passed at rates 0.780, 1.562, 1.260, 1.696 and 2.335; absolute LAF error stayed below 0.18 dB.
These prove clock separation and level conservation, not subjective first-person sound quality.

## Impact body and confirmed explosions

The armor voice previously gave its lowest partial roughly 5.8 seconds to lose 60 dB, and
higher modes retained much of that long decay. The replacement uses frequency-dependent
structural loss, `T60 = ln(1000) / (pi * frequency * eta)`, with `eta = 0.01` and the inherited
relative support/thickness/breach factors. Lower modes retain a longer body than upper modes.
This effective loss factor is an inferred timbre parameter, not a measured Tiger value.
The evidence, derivation and limitations are in
[the structural-decay note](research/IMPACT_STRUCTURAL_DECAY_20260929.md).

An internal explosion now requires a penetration with resolved
`DamageSoundContext.internal_detonation`; an ammunition catastrophe additionally requires
`catastrophic`. A projectile class, severe dry hit or casualty count alone cannot select it.
The sound and ear-pressure paths consult the same flag, based on the existing modeled non-HEAT
filler energy or live-ammunition outcome. A rejected ammunition outcome cannot leave an
invented catastrophic sound flag behind.

Confirmed explosive events receive a finite broadband pressure/rupture response and gas-release
tail. Their structural ring has authored relative weights of 0.28 for filler and 0.10 for an
ammunition catastrophe. Plate contact remains immediate; the air component follows the existing
pressure-front delay. The group receives one LAF calibration through the existing source-level
and reproduction law. Physical fragment contacts keep their separate trajectory-driven voices;
the impact synthesis adds no separate synthetic contact shower or independently calibrated explosion.
The pre-existing queued rack-blast and metal-rip voices remain separate from this impact group.
The burst bandwidth, envelope and layer balance are authored approximations, not measured
pressure histories, resolved detonation chemistry or a new armor-deformation simulation.

The selected own-gun recordings and hearing/reflex law remain unchanged. This does not establish
that every output event retains its old loudness: a different waveform has a different crest
factor and interaction with the master limiter. The measured dry-hit change below specifically
requires listening before acceptance.

## Master release and useful reduction telemetry

The limiter's single-precision exponential release could stall about 0.0023 dB below unity.
Its counter then marked every later quiet frame as limited. Control-envelope arithmetic now
uses double precision while retaining the existing ceiling, 200 ms release law and 70-frame
latency. The release reaches exact unity at the existing snap threshold.

Receipts now distinguish current reduction, deepest reduction, total limited frames, frames
over 1 dB reduction and frames over 6 dB reduction. Old total-limited-frame percentages cannot
establish sustained audible pumping because they include the stalled residue. Old deepest
reduction values remain meaningful. This is a precision/diagnostic correction; the tiny residue
was not an explanation for a hollow or small combat sound.

CPU checks passed: both selected limiter regressions and the master output/terminal-cut test.
The release regression checks the unchanged exponential timing, eventual unity, bit-exact quiet
output and separate threshold counters. These checks do not validate a complete encounter mix.

## Preserved calibration receipts and listening questions

The unmodified [baseline receipt](../captures/astra/sound_mass_20260929_baseline_receipt.json)
reports 23 events and 53 relations; the
[candidate receipt](../captures/astra/sound_mass_20260929_candidate_receipt.json) reports
24 events and 54 relations. Both declare their calibration checks passed. Values below are
the receipt measurements rounded to two decimals, with raw precision retained in those files.
They are unity-export measurements, not loudness-matched or normalized comparison files.

| Event | Baseline output LAF, dBFS | Candidate output LAF, dBFS | Baseline deepest limiter, dB | Candidate deepest limiter, dB |
|---|---:|---:|---:|---:|
| Close dry 76 mm side penetration | -25.40 | -29.10 | -5.70 | -7.70 |
| Heavy 122 mm penetration with filler burst | -22.20 | -21.45 | -23.78 | -18.09 |
| Cross-country engine, buttoned calibration | -34.64 | -34.76 | 0.00 | 0.00 |
| Cross-country engine through held listener reflex | -36.34 | -37.35 | 0.00 | 0.00 |
| Staged ammunition-outcome timbre at the 122 mm filler's ear level | No equivalent | -19.42 | No equivalent | -17.73 |

The dry hit loses 3.70 dB of measured output LAF and reaches 2.00 dB deeper reduction despite
the same requested physical ear level and reproduction target. Shortening a ringing tail can
change the perceived body as well as the measured level; this is an unresolved listening question,
not approved improvement or evidence of retained loudness. The 122 mm burst gains 0.75 dB output
LAF and its deepest reduction decreases by 5.69 dB, but still reaches 18.09 dB reduction. Do not
describe that event as free of strong limiting. These rows use `through_ears=false` unless the
listener-reflex row is explicitly named; the in-game ear state and concurrent voices add further
differences. The engine's nearly conserved pre-reflex LAF does not establish identical timbre
or identical output after the ear model.

The additional ammunition-outcome row isolates timbre at the existing 122 mm filler level.
It does not model or calibrate the pressure of a full rack, and cannot be compared as a resolved
combat outcome. No subjective listening approval has been recorded here.

## Terminal recorder correction

The preserved 34-second baseline duel reached actual KIA at 18.350000381469727 seconds but the
old recorder still accepted its staged discharge at 27 seconds. It froze `Game::update` while
continuing timeline/LMB callbacks, so the hidden postmortem shot incorrectly satisfied the
two-shot expectation under black and silence. The original media and receipt remain unchanged;
[the evidence note](../artifacts/immersion-20260929/duel-before-recorder-defect.md) labels this
defect. Its postmortem acceptance must not be used as valid input or gameplay evidence.

The recorder now latches terminal state before further action, map, LMB or gameplay-input
callbacks. Due skipped actions are recorded as rejected with `terminal_kia`; required rejected
actions fail overall acceptance. An ongoing input interval records its ignored remainder once.
The audio sample clock and ordinary 1.5-second picture/sound fade continue, then hold black and
silence. The two CPU regressions passed: callback suppression/failed two-shot acceptance with
the full audio-frame count, and preflight of the new fixture. These are recorder checks, not a
newly recorded first-person result.

The new [20-second terminal duel](../catalog/shader_reviews/pilot_cupola_t34_duel_terminal_20260929.v1.json)
retains the original setup and hatch at 0.2 seconds, own shot at 7 seconds and drive from 12–19
seconds. It requires exactly one own shot and an actual `AmmoDetonation` terminal outcome;
death is not injected. The old scenario is preserved as evidence of the former acceptance bug.

## Fatal acoustic arrivals

A separate runtime fault suppressed the entire fire-cookoff voice: `update_fire` queued it after
that frame's normal acoustic drain, and terminal death prevented any later `Game::update`.
The shared drain now runs in normal play and an audio-only terminal tick in both the native
window and the recorder. Already-emitted waves retain their delay, propagation, calibrated
source and listener processing. Gameplay and the fatal image remain frozen; pending waves
are cleared at the end of the existing fade. There is no new death sound or gain override. The existing queued rack and cookoff emissions use
fixed 3.67 kg and 0.71 kg TNT-equivalent estimates respectively; they are not derived from the
resolved live-ammunition inventory or a solved internal pressure history.

The corrected aimed-fire film records a CookOff waiting 5.242 ms at death, one terminal arrival,
and an empty final queue. Its video-only file is byte-identical to the prior film, as is every
pre-fatal audio sample. The previously missing voice now reaches the mix (peak -1.00045 dBFS);
black remains exactly silent. See [fire evidence](FIRE_SUPPRESSION_REVIEW_20260929.md), including
the unchanged survival failure. Two focused regressions cover delayed arrival through the
capture mixer, single emission, pause/retry isolation, and the bounded fade clock.

## Integration evidence

- Release compilation passed; binary hash is retained in `artifacts/immersion-20260929/final-binary.sha256`.
- `cargo test --locked --bin panzerrechnersystem`: **1,403 passed, 0 failed, 51 ignored**
  on the final pickup-corrected source; the additional check exercises real source mesh selection.
- Final CPU sound calibration: **24 events, 54 relations passed**, with event measurements identical
  to the earlier candidate receipt. Final receipt: `captures/astra/sound_mass_20260929_candidate_final_receipt.json`.
- Full `cargo test --locked` remains blocked by the handoff's missing `crate::resource` imports in
  `foot_ai_contract` and `combat_training`. Selected `physical_truth_contract` has 17 passes and
  the existing `glass_pipe` substring failure. The updated ledger wording passes its contract.
- `cargo fmt --all -- --check` still reports widespread existing style differences; an untouched
  `d7947c0` source sample reproduces them. No repository-wide formatting rewrite was made.
- `git diff --check` passes. The engine source is formatted; the local review-page builder compiles.
- The 20-second duel passes all seven state checks and required actions: one own shot, four enemy
  discharges, actual ammunition catastrophe at 18.3500004 s (matching baseline), and frozen terminal
  state. Pending TurretPop, TurretRip and the already-emitted enemy report all reach the listener;
  three terminal arrivals leave the queue empty. H.264 1920x1080 and stereo AAC/44.1 kHz are verified.
- Duel output peak is 0.891251 (-1 dBFS), with deepest gain reduction 25.23 dB; the restored rack
  voices now reach the limiter. New counters report 0.751 s above 1 dB reduction and 0.293 s above
  6 dB. These are stronger mixed-event peaks than the old incomplete mix, not a claim that all
  compression is resolved. Master totals include discarded warmup; threshold durations use 44.1 kHz.
- Player-view hammer recording: the original wrong-tool pickup failure is preserved; the fresh
  pickup-corrected fixture passes all 19 state checks. See the final film and evidence in
  [the repair record](REPAIR_INTERACTION_20260929.md).
- Fire/suppression: fixed synchronized films and actual outcomes are linked in the fire evidence;
  aimed survival remains failed, wrong-aim causal checks pass.
- Normal-speed player-view listening, especially the lower dry-hit body and strong heavy-burst
  limiting: **pending; no subjective approval claimed**.

Armor source contacts, a single online fragment consequence authority and real mesh deformation
remain separate work in [the armor/fragment plan](ARMOR_FRAGMENT_DEVELOPMENT_20260929.md).

The final interaction-only rebuild is SHA-256
`b35b740a86bb3e6c194773997fc14c6835dc30166f21517f4a111ccf1f800b33`.
The preserved `panzerrechnersystem-audio-verified` binary and its separate checksum identify
the prior sound/terminal build used for the final duel, calibration and audio-fixed fire films.
The subsequent pickup correction changes equipment selection and review gaze; it does not
change audio synthesis, calibration or terminal delivery. Fresh hammer media and fire pickup
evaluations identify that later build separately rather than relabelling the older recordings.
