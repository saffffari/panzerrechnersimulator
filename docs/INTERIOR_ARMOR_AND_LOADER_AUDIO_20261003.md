# Interior armor and loader audio correction

Implemented source and route corrections, pending coordinator build/live acceptance.

## Armor source

Own-hull shell strikes already played immediately through structure, without exterior airborne muffle. The deficient sound was in the source: a two-pole 2.6 kHz low-pass and weak upper modes dulled the plate attack. The corrected source retains low plate weight, adds four short, inharmonic upper contact modes, admits detail through a 5.2 kHz ceiling, and preserves more structural contact beneath confirmed explosive pressure. Actual energy, shell type, incidence, panel, station distance, bearing and seed remain authoritative. No per-hit upward normalization was introduced. Delayed airborne pressure and distant source propagation are unchanged.

These are authored material timbres, not measured Tiger modal frequencies.

## Actual loading routes

The live loader uses `audio_ammunition.rs` speed-driven extraction/ram motion and real cartridge collisions. Editing legacy extraction/drop one-shots alone would miss this route.

- Extraction and ram now excite multiple motion-driven upper friction bands alongside stick/slip body. Stop/reverse still use real speed telemetry and a release ramp.
- Real empty-case floor contacts retain a longer brass-wall ring plus compact floor knock. Live rounds retain a heavier, shorter response.
- Breech open/lock no longer substitute the unrelated, deliberately quiet hatch-stop recording. Their actual lever contacts use dedicated steel body and upper return, calibrated independently. Ordinary hatch routes remain intact.
- MainGunRecoil gets its own short steel return at the actual shot-time call, separate from muzzle report and extraction/drop. The report itself is not replaced by an entire soundtrack recording.

## Missing footsteps / floor movement

Metal footsteps were still dispatched, but inherited the quiet-paper class at 60 dBA before another approximately 9 dB cadence effort reduction. They now have an explicit 78 dBA close steel-floor contact estimate before effort. This is an authored calibration, not a historical measurement.

Floor contact audio previously required normal impact speed above 0.28 m/s, losing quiet rolling contacts. The gate now admits genuine grounded rolling taps (normal >0.035 m/s and tangent >0.16 m/s), or normal contacts >0.12 m/s. Support jitter alone is excluded. Each body has an 80 ms cooldown; each update plays at most the four strongest contacts.

Dropped-body angular damping was 0.72 per 120 Hz contact, erasing approximately 98% of spin within 0.1 seconds. It now uses continuous time-scaled angular/sliding losses. Real collision geometry, bounded queries and sleep/transform wake rules are retained. Chassis acceleration coupling into local-frame live rounds remains unmodeled; this change does not claim complete inertial floor dynamics.

## Evidence / acceptance

`captures/interior_armor_source_comparison_20261003_receipt.json` contains original reference paths, hashes and before/after isolated source metrics. The WAV files are extracted production `synth_impact` and `synth_mechanical` source, with no per-file gain normalization. They omit the live listener/master, pressure and casualty layers and therefore are not complete gameplay sound or subjective listening acceptance.

The production source extraction compiled with rustc and ran deterministic/finite/peak/zero-onset assertions. Cannon armor attack upper/body energy improved approximately 3.6–4.5 dB while maintaining lower plate body and event headroom. This frequency comparison is a diagnosis, not proof of realism.

Coordinator can produce a ten-second actual mixer loader sequence with:

```
KESSEL_LOADER_SEQUENCE_REVIEW_PREFIX=/home/alex/panzer/adaptive-audio-20261002/captures/loader_live_mixer_sequence_20261003 cargo test --release live_loader_motion_and_contact_sequence_review -- --nocapture
```

This is an authored controlled timeline using actual `Audio`, continuous loader motion and cartridge contact sources. It excludes muzzle report and does not claim captured player timing. Source and routing tests also cover recoil decay, real rolling contact gating, invalid/jitter rejection, casing/live-round distinctions, motion stop/reverse and existing structural/pressure timing.

## Complete live-mixer fixture and MG floor cases

`complete_gun_reload_floor_and_steps_live_mixer_review` now plays the actual loaded interior main-gun bank alongside shot-time recoil, breech contacts, extraction/ram motion, real cartridge contact APIs, metal footsteps and dedicated MG-case contact through `Audio`, hearing and master. It writes effects-only, effects-with-owned-music and score-probe WAVs plus per-event measured windows. The owned music is selected deterministically from enabled imported music cues and played on the real Score bus at a declared quiet fixture gain. Control/contact APIs exercise the actual interaction duck. This fixture is a controlled sequence, not gameplay timing, and no listening acceptance is claimed.

Run the coordinator-built test binary with `KESSEL_COMPLETE_INTERIOR_AUDIO_REVIEW_PREFIX=/home/alex/panzer/adaptive-audio-20261002/captures/complete_interior_live_mixer_20261003_v1` and filter `complete_gun_reload_floor_and_steps_live_mixer_review`. Preserve previous prefix outputs.

The mounted MG previously created no spent case body at all. `mg_case_runtime.rs` now emits one real brass case per authoritative mounted-MG round, into a fixed pool of 48 retained bodies. Dimensions are 58×12 mm and mass is an authored 12 g approximation. Collision uses the same retained source triangles/bounded dropped-body solver as main-gun cartridges; genuine normal/rolling floor contacts call a dedicated small-case timbre. At most four sounds play per update, with a 100 ms per-body cooldown. There is no artificial delayed floor clank. The receiver coordinates and throw are declared game-rig approximations pending named source-mesh port tags. The oldest represented case is recycled at capacity; this is not an unbounded persistent brass inventory.

Gun elevation was still wired to actual pitch displacement. Its very steep `75 + 20log(rate)` effort law hid gentle motion. It now uses an explicit `78 + 10log(rate)` energy law while retaining actual movement gating, position and enclosure transfer. Turret traversal callback is also present; engine-off hydraulic gating remains deliberate, and an independent hand-traverse source is not introduced by this pass.

Both accepted turret-floor movement and exterior support contacts still dispatch footsteps; the missing audibility was the quiet-paper calibration and masking, rather than a removed callback. The test fixture exercises the actual metal-footstep key/Score protection but does not by itself prove player movement acceptance.

### Close handling reproduction correction
The complete production-mixer fixture exposed a large remaining mismatch: own shot peak
-2.12 dBFS, extraction -22.3, floor case -38.7, ram -40.9, metal steps -40.2,
MG floor case -45.4. The supplied `main_gun_and_reload_perfect.wav` has half-second
peak windows between 0 and -12.8 dBFS across its 6.592 seconds. These window statistics
do not identify individual events or establish physical sound pressure, but show that the
reference keeps handling much more prominent than the fixture.

`play_handling_at_ear` therefore applies a named authored +14 dB reproduction gain after
physical ear calibration to real 88 mm/MG floor contacts. Loader motion uses +10 dB
reproduction gain and +6 dB relative ram-friction rendering. Source dBA estimates,
collision energy, motion gates, hearing processing, death fade and shared limiter remain
in the actual route. This is an artistic reproduction choice, not a historical measurement.
No new fixture run or listening acceptance is claimed by this source edit.

### Regression corrections (source staged)
Local synthesized mechanical bearings now retain both ears rather than using full world-source
panning. Saturation is applied to stereo magnitude, so turning one's head does not change the
energy/identity of a local contact. The original opposite-ear, energy and symmetry guards remain.
Metal footstep +14dB handling reproduction is now a digital gain AFTER physical tone mapping;
the old implementation mistakenly added14dB to the source ear level before nonlinear mapping.
Its fixture explicitly accounts for the named reproduction choice while nonmetal levels stay
on their former physical curve.
Catastrophe test exposed >21dB limiter reduction at10m. The synthesized pressure waveform now
includes a short aperiodic420Hz low-passed body beneath its original onset to reduce its crest
before measured LAF calibration. Physical emission and propagation are unchanged, and the
existing severity/headroom guards have not been relaxed. Test/listening acceptance remains pending.
