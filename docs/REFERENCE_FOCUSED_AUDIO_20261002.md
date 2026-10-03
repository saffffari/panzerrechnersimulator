# Focused audio references — 2026-10-02

**Primary whole-cycle target: `main_gun_and_reload_perfect.wav`, per the user's
latest selection.** `main_gun_firing_inside.wav` remains the secondary, specifically
interior-discharge reference. An exterior gun report is not a substitute for either.
`incoming_tank_round_non_prenetrating.wav` is the primary requested nonpenetrating
interior-hit target (file spelling retained). The final `mg_inside.wav` attachment
is treated as the primary inside-MG target from context, separately from the earlier
mixed front-MG/armor-impact clip. These labels state the intended use, not verified
historical source identities or physical impact outcomes.
The newer `heavy_incoming_round_non_penetrating.wav` supplies a paired heavy-hit
target rather than replacing the earlier nonpenetrating clip. `reloading.wav` is
the primary standalone-reload target, complementary to the complete gun/reload
sequence; “standalone” describes its intended use, not verified source isolation.

Nine original WAVs were fully decoded and measured without alteration. This pass
did not play them, change runtime sounds, build the game, or run the GPU. The
waveforms/spectrograms below were visually inspected; that is not a listening
judgment. File names describe the user's intended subjects, not independently
verified event identities.

## Provenance and integrity

Treat these excerpts as film/mixed presentation references for the desired
relative weight, timing and separation. Their original recording/mixing chain,
source identity and any music/dialogue content have not been established. They
are distinct from the user-described real Tiger museum recording
`/home/alex/Recordings/tiger1_tank_museum_sound.wav`, which is the stronger
source-identity reference for engine/track character but still has an unknown
microphone, distance, gain and processing chain. Neither class establishes
absolute physical SPL. These WAVs have not been admitted to shipping assets.

All nine are **48,000 Hz, stereo, signed PCM16 little-endian**. FFmpeg full
decode with `-xerror` succeeded, every sample is finite, and the resulting float32
samples equal the WAV's PCM16 samples divided by 32768 exactly. Original SHA-256
hashes were checked again after analysis.

| Original `/home/alex/Recordings/` file | Frames/channel | Duration | Bytes | SHA-256 |
|---|---:|---:|---:|---|
| `front_mg_and_interior_impact_sound.wav` | 403,456 | 8.405333 s | 1,613,868 | `a2fe9e381d948fe6544716ab3cf695fec41c35b10aeee2323982fd1f4ddb043e` |
| `incoming_tank_round_miss.wav` | 146,432 | 3.050667 s | 585,772 | `6cc6bc50fa15f60f4cdad9091538a36b3fa18642a5e49f0e7362b9b21d6c98ad` |
| `engine_and_hatch_closing.wav` | 108,544 | 2.261333 s | 434,220 | `706067786d0b398094ec4c309a2f909d67b50ecacda87d1841a9d195b329d69b` |
| `main_gun_firing_inside.wav` | 185,344 | 3.861333 s | 741,420 | `9a2f63e6641b39f6bb27b49b9139cf900aada109f749e7172ec9913fd28a865d` |
| `main_gun_and_reload_perfect.wav` **primary** | 316,416 | 6.592000 s | 1,265,708 | `d26573c998d81aff02474bcd653a858fa4e6f7614beb8fe0a99e4a6eef8ebe74` |
| `incoming_tank_round_non_prenetrating.wav` | 107,520 | 2.240000 s | 430,124 | `aec5c8a0bc6e9f5bc2dfcffde861765233cb0226e26ba7c0296c0bf2ef95401d` |
| `mg_inside.wav` | 174,080 | 3.626667 s | 696,364 | `b29f89ba3858d34bdf96d74f5904291d3fe9214cfe4cd72855a825d72bf715e0` |
| `heavy_incoming_round_non_penetrating.wav` | 340,992 | 7.104000 s | 1,364,012 | `5be7f25e9ca96a27f25f7706186326b213e0f2b0bc03289ba8f8365c10892105` |
| `reloading.wav` | 273,408 | 5.696000 s | 1,093,676 | `74358e9acb400833c799ea37462eeac534b7c1c43461c7f73443164fc9e045ce` |

## Measured structure

RMS uses mean stereo-channel power. Peak is the largest native PCM sample,
**not oversampled true peak**. Full-scale counts count individual channel samples
at +32767 or -32768; these establish contact with the digital rails, not the cause
or perceived severity of any distortion. Band percentages use demeaned whole-clip
FFT energy averaged across channels and normalized over **20–24,000 Hz**. They
are not equalization targets and cannot identify which source supplies the bass.

| Excerpt | Peak dBFS | RMS dBFS | Crest dB | Full-scale samples (% of channel samples) | 20–80 Hz energy | 80–250 Hz | 250–1000 Hz | 1–4 kHz | 4–24 kHz |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Front MG / interior impact | 0.00 | -9.64 | 9.64 | 1,263 (0.1565%) | 69.53% | 17.59% | 9.11% | 3.27% | 0.49% |
| Incoming round miss | 0.00 | -10.20 | 10.20 | 119 (0.0406%) | 62.04% | 21.66% | 9.71% | 6.22% | 0.37% |
| Engine / hatch | 0.00 | -8.91 | 8.91 | 68 (0.0313%) | 38.01% | 52.07% | 7.34% | 2.26% | 0.31% |
| Interior main gun | 0.00 | -10.25 | 10.25 | 24 (0.0065%) | 61.49% | 20.05% | 12.43% | 5.33% | 0.70% |
| Main gun + reload **primary** | 0.00 | -14.68 | 14.68 | 17 (0.0027%) | 41.31% | 21.88% | 21.79% | 11.57% | 3.44% |
| Nonpenetrating interior-hit target | 0.00 | -9.03 | 9.03 | 176 (0.0818%) | 67.64% | 20.74% | 8.03% | 2.85% | 0.72% |
| Inside-MG target | 0.00 | -9.32 | 9.32 | 1,171 (0.3363%) | 81.53% | 11.07% | 4.76% | 1.92% | 0.71% |
| Heavy nonpenetrating-hit target | 0.00 | -10.76 | 10.76 | 240 (0.0352%) | 68.55% | 19.34% | 9.73% | 2.20% | 0.18% |
| Standalone reload target | 0.00 | -13.29 | 13.29 | 10 (0.0018%) | 41.09% | 28.11% | 19.50% | 8.60% | 2.70% |

All contain substantial low-frequency energy alongside intermittent broader-band
structure; none resembles stationary white noise in the measured spectrum.
However, a flat whole-clip EQ match would fold the recording's mix, background and
unknown processing into every game source. Do not use these figures to increase
source SPL, invent low-frequency energy, set weapon cadence or replace propagation.
Left/right correlation is 0.808, 0.693, 0.833, 0.718, 0.843, 0.748, 0.933, 0.726 and 0.922 respectively; none is duplicated
mono. This does not establish spatial placement or motion of an individual source.

### Primary: main gun and reload, complete interior sequence

The latest selection has considerably more relative mid/upper-band energy than
the first three references, as well as more separation between strong events and
quiet intervals. Its 1–24 kHz share is 15.01%, versus 3.76%, 6.59% and 2.58% in
those three. That is evidence against reducing this request to more bass or a
single louder shot. Whole-clip differences also include duration/background/mix;
they are not proof of any specific mechanism's spectrum.

These **provisional analysis phases** decompose an observed envelope, not verified
gun/reload labels. The hypothesized attack/body/tail correspondence must be checked
by listening before timing authored events to it. Contact/ram/drop order cannot
be established from RMS peaks alone.

| Source interval | Measured signal / candidate review purpose | RMS dBFS | Sample peak dBFS |
|---|---|---:|---:|
| 0.80–1.40 s | Pre-event context; narrow bands and intermittent changes | -15.55 | -1.59 |
| 1.40–1.50 s | Rapid rise; candidate attack window | -9.99 | -0.18 |
| 1.50–1.95 s | Sustained stronger low/broadband body | -9.71 | 0.00 |
| 1.95–2.60 s | Early decay region | -15.95 | -4.07 |
| 2.60–3.50 s | Quieter late interval; preserve this contrast | -23.15 | -11.17 |
| 3.60–4.65 s | Renewed discrete broad-band activity; candidate mechanism group A | -13.55 | approximately 0.00 |
| 5.20–5.60 s | Short renewed activity before the final strong group | -14.80 | -2.20 |
| 5.60–6.35 s | Stronger group B; candidate handling/contact review | -11.58 | 0.00 |
| 6.35–6.59 s | Final falling envelope | -20.44 | -7.05 |

The 1.50–1.95 s body exceeds the 2.60–3.50 s quiet interval by **13.44 dB RMS**;
group B exceeds that interval by **11.57 dB**. These within-recording contrasts
are useful A/B questions. They are not physical source-level differences.

![Primary complete-cycle signal](../captures/focused_audio_main_gun_and_reload_perfect_20261002.png)

The whole clip is short enough to preserve without trimming or editing. An exact
byte-identical local review copy is available as
`captures/focused_reference_main_gun_and_reload_perfect_20261002.wav`.
It is a listening reference, not an installed/shipping gun or reload sample.

### Secondary: main gun firing inside

The major visible rise around 1.14 s supplies a second provisional decomposition:

| Source interval | Analysis purpose, awaiting semantic audition | RMS dBFS | Sample peak dBFS |
|---|---|---:|---:|
| 0.75–1.14 s | Pre-event context | -15.12 | -3.70 |
| 1.14–1.20 s | Candidate attack, 60 ms | -10.76 | -0.15 |
| 1.20–1.54 s | Candidate body, 340 ms | -7.15 | 0.00 |
| 1.54–2.34 s | Candidate tail, 800 ms | -8.82 | -0.03 |
| 2.34–3.86 s | Late mixed activity; not isolated mechanism audio | -10.66 | 0.00 |

The later waveform contains continuing activity, so do not fit a pure decay or
infer an interior RT60 from this mixed excerpt. Its exact local review copy is
`captures/focused_reference_main_gun_firing_inside_20261002.wav`.

![Secondary interior gun signal](../captures/focused_audio_main_gun_firing_inside_20261002.png)

### Primary nonpenetrating interior-hit target

The original misspelling `non_prenetrating` is retained everywhere. The clip is
2.240 s and already has strong signal near its start; it does not supply a quiet
pre-event baseline. RMS is -11.27 dBFS over 0–0.2 s, -8.24 over 0.2–0.8 s,
-7.50 over 0.8–1.5 s and -11.67 over 1.5–2.24 s. Broad-band changes and narrow
bands coexist above a strong low-frequency bed. Treat the last region as lower
activity, not an established isolated armor-ring decay.

Candidate signal-peak windows: 0.195–0.595, 0.475–0.875, 0.665–1.065,
0.945–1.345, 1.125–1.525 and 1.345–1.745 s. They do not establish six hits.
Keep this reference distinct from the incoming **miss**, a penetrating internal
burst and a catastrophic ammunition event. A nonpenetrating impact can excite
the structure; its actual game outcome must not create penetration/catastrophe
effects solely to resemble the supplied mix.

![Nonpenetrating interior-hit target signal](../captures/focused_audio_incoming_tank_round_non_prenetrating_20261002.png)

### Paired heavy nonpenetrating-hit target

This 7.104 s excerpt begins with substantial signal, so there is no reliable
pre-hit quiet baseline. RMS is -8.19 dBFS over 0–0.4 s, -7.71 over 0.4–1.2 s,
-8.96 over 1.2–3.2 s, -11.70 over 3.2–4.2 s and -15.55 over 4.2–6.6 s.
The early sustained group and the lower later region provide a useful body/tail
contrast; renewed changes around 5.6 s and near the end mean the full duration is
not an isolated exponential decay or a measurable cabin impulse response.

Candidate signal-peak windows: 0.000–0.345, 0.135–0.535, 0.725–1.125,
2.165–2.565, 2.565–2.965 and 2.765–3.165 s. Keep the full 3.2–6.6 s region
for later-tail/context audition; peak-only snippets would omit it.

Its 20–250 Hz share is 87.89%, close to the earlier nonpenetrating target's
88.39%, while its whole-clip RMS is **1.73 dB lower**. The heavy label therefore
must not be implemented as an arbitrary multiplier derived from these file levels.
Compare onset, structure, duration and decay in the pair while game collision
energy/outcome remains authoritative. Neither clip alone verifies nonpenetration,
caliber, vehicle type or a particular structural resonance.

![Heavy nonpenetrating-hit target signal](../captures/focused_audio_heavy_incoming_round_non_penetrating_20261002.png)

### Primary standalone reload target

The 5.696 s recording provides several separated signal groups for detailed
handling audition, without using the gun transient in the complete-cycle reference
as the analysis anchor. The whole clip has 19.50% of its measured band energy at
250–1000 Hz and 11.30% above 1 kHz. Narrow moving bands and broadband activity
overlap; the waveform alone cannot assign any group to extraction, case floor
contact, ram friction, latch, speech or background.

Candidate RMS-peak windows: 0.000–0.375, 0.255–0.655, 0.565–0.965,
1.315–1.715, 2.895–3.295 and 3.695–4.095 s. Also audition the visually distinct
groups around 2.1–2.65, 2.75–3.45, 3.55–4.05 and 5.15–5.50 s with their quiet
gaps intact. These are measured/visible review windows, not confirmed mechanical
event labels. The actual game extraction, contact and ram hooks documented below
should be auditioned separately against listener-identified matching phases,
then combined using actual player movement and contact timing.

![Standalone reload target signal](../captures/focused_audio_reloading_20261002.png)

### Primary inside-MG target

This 3.627 s clip contains repeated vertical spectral changes, strongest in a
group around 2.25–3.0 s, on top of a sustained low-frequency component. RMS is
-11.84 dBFS over 0–0.9 s, -11.59 over 0.9–1.35 s, -9.06 over 1.35–2.35 s,
-6.87 over 2.35–3.1 s and -9.72 over 3.1–3.626 s. The event group rises about
5 dB above the opening interval. Without source isolation/listening, the measured
bass cannot be assigned exclusively to the MG rather than the vehicle or mix.

Candidate signal-peak windows: 1.305–1.705, 1.495–1.895, 2.305–2.705,
2.485–2.885, 2.675–3.075 and 2.925–3.325 s. The peak selector enforces a minimum
180 ms separation, so **these markers cannot measure weapon cadence**. Retain
the historically selected runtime cadence and compare timbre/continuity through
the actual shot schedule. Use the earlier front-MG clip for overlapping-contact
mix checks; use this selected clip for the inside-MG character target.

![Inside-MG target signal](../captures/focused_audio_mg_inside_20261002.png)

### Front MG / interior impact

The first 0.6 s contains repeated short vertical spectral bands and has -6.71
dBFS RMS; 0.6–2.1 s drops to -11.11 dBFS. The envelope rises around 2.15 s;
2.15–5.8 s measures -8.29 dBFS, followed by -14.56 dBFS over 6.4–8.3 s. Narrow
horizontal bands coexist with broadband changes. These can guide an A/B of
transient/body/tail separation, but do not prove a shot count or distinguish
mechanism, hull resonance, music or speech.

Candidate **signal-peak** review windows: P1 0.000–0.355 s; P2 0.135–0.535 s;
P3 0.325–0.725 s; P4 2.475–2.875 s; P5 2.675–3.075 s; P6 5.495–5.895 s.
Overlaps are intentional; these are local RMS maxima, not six asserted impacts.

![Front MG and interior-impact signal](../captures/focused_audio_front_mg_and_interior_impact_sound_20261002.png)

### Incoming round miss

Measured power rises across the excerpt: 0–0.5 s is -18.37 dBFS, 0.5–1.3 s
is -10.63 dBFS, and 1.3–2.0 s is -8.43 dBFS. There are changing narrow-band
tracks in approximately the 1–3 kHz region around 1.1–1.9 s within a broad
low-frequency bed. A later 2.0–3.0 s region is -9.54 dBFS with brief interruptions.
This gives a candidate build/change/aftermath shape for audition. It does **not**
establish projectile Doppler, a shockwave, speed, miss distance or impact time.

Candidate **signal-peak** windows: P1 1.345–1.745 s; P2 1.835–2.235 s;
P3 2.185–2.585 s; P4 2.365–2.765 s; P5 2.615–3.015 s; P6 2.825–3.051 s.
Also audition 0–1.3 s as context; peak-only snippets would omit the build.

![Incoming-round-miss signal](../captures/focused_audio_incoming_tank_round_miss_20261002.png)

### Engine / hatch

The short clip grows from -14.28 dBFS over 0–0.2 s to -11.87 dBFS over
0.2–0.6 s, -8.71 dBFS over 0.6–1.3 s and -7.69 dBFS over 1.3–2.2 s. A
continuous low-frequency bed remains beneath broader-band changes, including
around 0.6–1.2 s. This supports reviewing continuity beneath a mechanical event;
the waveform does not independently locate a hatch contact or establish engine RPM.

Candidate **signal-peak** windows: P1 0.555–0.955 s; P2 0.755–1.155 s;
P3 1.045–1.445 s; P4 1.225–1.625 s; P5 1.435–1.835 s; P6 1.645–2.045 s.
Use the full 2.261 s clip when judging the continuing background.

![Engine and hatch signal](../captures/focused_audio_engine_and_hatch_closing_20261002.png)

## Targeted acceptance sequence for the game

The following are proposed tests, **not passed results**. First identify useful
events by listening to the originals. Then capture game scenarios with an event
log, consistent master/output settings, and known listener/vehicle state. A second
audition copy may be level-matched for timbre judgment; retain the unmodified
capture for relative-level and propagation checks. Do not judge success by making
the candidate louder or matching these whole-clip frequency percentages.

| Scenario | Technical evidence | Listening acceptance |
|---|---|---|
| **Primary: seated interior main gun through a complete reload cycle** | Capture the actual interior report, recoil, breech opening, extraction friction/release, real case contacts, ram friction/seating and breech lock as separate stems plus their combined output. Log real sequence times. Keep the same headset/hearing state, engine background and output settings for current/candidate. | A forceful compact shot with an identifiable body and decay, followed by distinct friction, hollow case ringing/contacts and seating/lock events. The quiet interval matters. Use the latest full-sequence reference; do not substitute an exterior boom or make every phase the same noise burst. |
| **Primary nonpenetrating interior strike**, then miss/penetration/catastrophe controls | Log collision point, nonpenetration result, time of contact, station/headset and transmitted/contact sources. No internal explosive burst unless the actual outcome creates one. | Distinct compact structural attack/body/metal response, with a readable difference from a nearby miss, penetrating burst and catastrophic event. Audition the supplied nonpenetrating target as its own category. |
| **Heavy and lighter nonpenetrating pair** | Record each impact's actual energy, angle, contact position and nonpenetration result; hold listener and output state constant. | Different structural weight/body and aftermath without turning the heavier nonpenetration into an internal explosion. Do not infer a loudness multiplier from the differently mixed source files. |
| **Standalone reload mechanisms**, then complete cycle | Log signed extraction/ram travel, release, actual case contacts and lever state; provide isolated mechanism stems followed by the full combined output. | Distinguishable scrape/release/ring/contact/seating details, with gaps and varied texture. Use `reloading.wav` to identify those phases by listening before assigning any spectral peak a mechanism name. |
| Tiger MG alone, then MG plus incoming armor contacts; closed and open hatch | Actual per-round emission timestamps and audio arrivals; 800 RPM means 75 ms intervals. Preserve phase across low-FPS catch-up. Separate local receiver/mount energy from propagated exterior muzzle report and actual impact arrival. | Short distinct pressure/mechanical attacks, a convincing body beneath them, and enough decay space for impacts to remain identifiable. No sustained hiss replacing all events; no repetitive hollow comb tail. Do not derive RPM from this mixed excerpt. |
| A round passes without hitting the tank, then comparable shots terminate in soil and armor at known positions | Projectile history, closest approach, terminal event and propagation arrival times. Keep trajectory/supersonic conditions authoritative. The vehicle-hit cue must not fire on a miss. | A passing event changes over time and position; terminal report remains distinct if one occurs. Direction and relative distance remain readable. Tonal motion must not become the same decorative whistle on every trajectory. |
| Engine running while the hatch slowly closes and while it is shut briskly | Continuous engine RPM/load state; hinge travel/contact timing; actual hatch state and listener transfer. Capture before/during/after without restarting the engine voice. | Engine presence continues underneath the mechanism; latch/contact weight follows the action. Closure changes exterior sound without muting internal structure, popping or leaving a permanently muffled unrelated effect. |
| Engine idle, loaded hill climb, neutral rev, movement and turn | Actual RPM, torque/load, gear and separate left/right belt speed; compare museum reference separately. | Engine speed and load have distinct cues; track grind follows belt motion and turn demand instead of continuing unchanged at rest. Do not infer real engine pitch from these short film/mixed excerpts. |

For each capture record native FPS, underrun/late-submission evidence if available,
master limiter activity, station, hatch position, listener distance, and selected
audio source variants. A CPU scheduling test or a plotted spectrum is not proof
that the first-person mix sounds correct.

## Current interior-gun and reload route audit

No literal `gun88_hull` definition occurs in the current HOTFIX `src`, `catalog`,
`assets`, `tools` or `docs` search. The actual interior route is:

1. `src/game.rs::play_player_main_gun_report` chooses `PlayerGunInt1/2` when the
   crew head is inside, and the appropriate `OWN_88_BUTTONED` or hatch-open ear
   level. `listener_hearing.rs::play_and_hear_at_ear` forwards the real source
   onset/envelope to hearing and then `Audio::play_at_ear`.
2. `src/audio.rs` maps these variants to `ProceduralSound::GunInterior(1/2)` in
   the default fallback path. The legacy `main_gun_fire_int_1/2.ogg` recordings
   load only with explicit noncommercial opt-in and its layer conditions; loaded
   metadata must establish which route the actual capture uses.
3. `src/acoustics.rs::synth_gun_punch(true, seed)` uses a 0.52 s authored cannon
   source with a compact pressure pulse, 49/118/245/287 Hz contributions and a
   short upper crack. Its noise/body tail has a nominal 65 ms decay with a
   0.20–0.36 s cutoff. `synth_procedural` then adds `STEEL_INTERIOR`: authored
   RT60 0.38 s, wet 0.34, damping 0.35. This is a code-defined design, not a
   measured reconstruction of the supplied primary recording.
4. The game separately adds `MechanicalCue::MainGunRecoil`: 0.72 s with fixed
   contact offsets 0, 55, 360 and 610 ms. This may obscure or duplicate a desired
   mechanical envelope; isolate it in A/B before adding gain or more samples.
5. The old private feedback `gun_interior` entry is a slowed generic explosion
   (0–2.4 s, pitch 0.72, 1.5 kHz low-pass, bass factor 2.2), but
   `audio_feedback.rs::replacement_key` no longer selects it for the own gun.
   **The global reproduction exposure still uses its old 15.69 dB crest constant,
   and `own_gun_recording_crest_matches_the_exposure_constant` still measures that
   obsolete reference bank entry.** Measure the active route's LAF/peak and
   limiter behavior before judging a gain change. Do not blindly re-anchor global
   exposure because that changes every source.

Reload is partly continuous and partly event-driven, so it can be captured without
baking a film-timed mechanical soundtrack over unrelated player actions:

- Lever threshold crossings: `game.rs` around 15800, `BreechOpen/BreechLock`.
- Manual case release: around 15849, `CasingExtract`; the automatic/fumble path
  around 17575 also emits its completion cue.
- Ram seating: around 15908 and the service path around 17536, `ShellRam` and
  closure where applicable.
- Actual extraction/ram travel: `loader_contact_motion` around 7618 and
  `set_loader_motion` around 16514. `audio_ammunition.rs` follows signed chamber
  travel with continuous friction, currently authored 1073/733 Hz resonators.
- Real case/round contacts: `play_cartridge_contacts` around 28786. Empty cases
  have 431/1097/2371/4133 Hz modes and a 0.48 s voice; live rounds use lower,
  shorter modes. Contact energy sets level; contacts are not scheduled from the
  film. These frequencies are authored estimates, not measured brass acoustics.

For a **current audio-only baseline**, the existing command
`impact-mix-review <prefix>` renders `own_gun_buttoned`, hatch-open/head-out, and
hearing/engine-bed variants through the live mixer. It does not capture the whole
reload sequence or all of `play_player_main_gun_report`'s extra mechanism layer.
The whole-cycle comparison therefore needs the actual gameplay event hooks or an
explicitly labeled CPU fixture invoking all those hooks; a dry `soundbank` clip
alone cannot stand in for the player experience. No capture command was run by
this analysis pass.

## Reproducible outputs

- `artifacts/runtime/reference_focused_audio_20261002.json`: exact metadata,
  hashes, full decode receipt, per-window metrics and figure paths.
- `artifacts/runtime/reference_focused_audio_analysis_20261002.py`: CPU-only
  reproduction using Python/NumPy/Matplotlib and FFmpeg; originals are read-only.
- Nine flat `captures/focused_audio_*_20261002.png` files: 10 ms native PCM
  min/max envelopes, stereo-power RMS and Hann STFT (2048 samples, 10 ms hop,
  fixed -80 to 0 dBFS per-bin amplitude scale, 23.4375 Hz FFT-bin spacing).

- `artifacts/runtime/reference_focused_audio_review_copies_20261002.json`: hashes
  for two byte-identical local review copies, totaling 2,007,128 bytes; full
  originals are only 6.592 s and 3.861 s, so no timing, level or tail was trimmed.

No extracted shipping samples, runtime edits, GPU runs or subjective acceptance
claims were made by this analysis pass.
