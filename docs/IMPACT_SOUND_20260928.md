# Impact sound overhaul — 28 September 2026

The guide this pass implements (Panzerrechnersystem Impact Sound Guide, 28 Sep 2026):

> A hit feels deafening when the world drops out just before it, four layers land together, and
> the player's hearing is taken away after. Contrast, layering and consequence do that work; raw
> level can't, because full scale is a hard ceiling.

Branch `claude/sound-overhaul-20260928`, built on the atmosphere pass (`claude/atmosphere-20260925`,
itself a snapshot of the shader-graphics winter work). All three principles apply to a strike on
the listener's own Tiger. Everything below is a mix/listener presentation choice driven by the
authoritative simulation; none of it is an acoustic measurement of a Tiger, and no speaker or
headphone audition is claimed.

## What was wrong

Measured on the live mix with the staged-hit fixture (`cargo run -- impact-mix-review <prefix>`):

- **The concussion duck landed on the hit itself.** Concussion was implemented as a master-volume
  change applied on the next frame, and the hit voice sat on that bus. A heavy hit played at full
  level for about one frame (16–30 ms) and then dropped to 0.085 (heavy) or 0.035 (catastrophic).
  The close 76 mm penetration's first 150 ms came out **1.3 dB quieter** than the world bed before
  it; the 122 mm was a 20 ms blip. Heavy hits read as clicks.
- **No contrast before the hit.** The world bed ran at full level right up to contact.
- **Hearing loss was level only.** Continuous beds (engine, wind, traverse) were only turned down;
  only newly started world one-shots were muffled, and distant parametric gun reports skipped even
  that. The high-frequency share of the world was unchanged after a concussion.
- **The strike bloomed instead of landing.** Once un-ducked, the layers were spread over 30–90 ms:
  the steel modes ramped in over 3–12 ms, the cabin pressure wall peaked at 41 ms (+ air delay) with
  its rebound at 95 ms, the panel flex started at 8 ms and rang for ~150 ms. The loudest 10 ms of a
  hit came 20–30 ms after contact, 10–12 dB above its first 10 ms.
- **15 dB of headroom was unused** on a close 76 mm penetration (peak 0.18).
- A dismounted crewman was concussed by strikes on the parked Tiger (the retired tracer path had
  guarded this; the live projectile path had lost it).

## Contrast: the world drops out just before

`Game::predict_player_armor_strike` → `probe_strike_on_hull` (game.rs). Every frame, each in-flight
enemy cannon round that passes a straight-line gate (closest approach within 7 m of the hull inside
the look-ahead) is **cloned** and swept with its own analytic flight law, wind and world contacts
(everything except the player's hull), and against the hull displaced by its current velocity.
It is read-only: it cannot select or alter an outcome. When a strike is predicted:

| | |
|---|---|
| Look-ahead | `PRE_IMPACT_LEAD_S` = 0.20 s |
| Fall | even in dB from 0.20 s to 0.07 s before contact |
| Hold | floor from 0.07 s to contact |
| Depth | 10–26 dB: `pre_impact_depth_db(kinetic energy, distance to the listener)` |
| Release | 60 dB/s once no strike is predicted (after it lands, or if the simulation resolves it differently) |

Only world voices follow it; the strike plays on the impact bus. The player's own main-gun report
holds the drop off for 0.35 s, so a shot fired in that window is never swallowed (the incoming
strike still lands with its layers and takes the hearing after). A supersonic round gives no
physical acoustic warning, so this is a named presentation choice read from authoritative state.
Limits: hull rotation over the 0.2 s window is ignored; an AP round that will transit a wall first is
not predicted (no drop, the strike still lands normally); small arms never trigger it.
Tests: `pre_hit_probe_agrees_with_the_simulation_it_previews` (hit/miss and timing match the 240 Hz
simulation within one step, stationary and at 12 m/s, and hull motion changes the answer for some
aims); `pre_hit_probe_ignores_far_blocked_and_missing_rounds_and_honours_the_fixed_step_lag`.

## Layering: four layers land together

`synth_impact` (acoustics.rs) is now four explicit layers, all starting at contact:

1. **Contact** — the edge wavelet and grit (brightness from velocity/hardness), raised about 9 dB
   (edge) and 7 dB (grit).
2. **Body** — `add_plate_body`: a compact 180–520 Hz band-noise knock (≈1 ms rise, 19–34 ms decay
   by calibre, over within six decay times) plus a coherent knock at contact.
3. **Weight** — `add_strike_weight`: a Friedlander-shaped panel thump (and on a breach the punched
   plug) via `add_blast_pulse`, which carries its energy from the moment it starts instead of after a
   symmetric pre-lobe. The panel flex now rises with the contact (2 ms, 1.5 ms attack, τ 30 ms).
4. **Ring** — the steel modes, now with a contact-time attack of 0.8–3 ms instead of 3–12 ms, so
   every mode starts in phase with the strike; about 1.4 dB lower so its beats sit under the onset.

The cabin pressure wall for a penetration is a Friedlander blast front starting at the strike (plus
the physical air delay) instead of a wavelet centred at 41 ms with a 95 ms rebound; fixture shocks
are kept as a quieter tail. Ricochets keep the ring as their character (spang vs bang): every band
still starts at contact, but the ring's beats carry them.

Level: `VOL_HIT_ON_PLAYER` = 2.2 on the impact bus, with a fixed −1 dBFS ceiling (`IMPACT_CEILING`,
linear attenuation only — no limiter, no distortion). The energy/proximity loudness law is unchanged;
heavy calibres meet the ceiling, so their extra weight must come from the deeper pre-hit drop and
the heavier hearing loss. The trauma layer (ear pressure/fatigue) starts 25 ms after the strike's
onset (`after_strike_onset`, the offset the review films already used) so the strike lands clean at
the ceiling.

Test: `strike_layers_land_together_before_the_ring_carries_the_tail` (every band reaches half of its
first-150 ms maximum within 3 ms; for penetrations the first 10 ms is the loudest 10 ms; the ring
still carries the tail). All existing voice tests (darkness, determinism, ring decay, direction,
fragment subordination) pass unchanged.

## Consequence: the player's hearing is taken away after

`audio_ear.rs`: every voice now ends in a per-sample listener stage (it replaces the old silence
gate and still owns the sample-exact KIA cut). Three buses:

- **World** (engine, wind, loops, world one-shots, scene emitters, mechanisms): pre-hit contrast ×
  concussion level, plus a per-ear two-pole low-pass for the temporary threshold shift.
- **Impact** (the strike on the listener's own vehicle): exempt from the pre-hit drop; untouched for
  120 ms so all four layers land, then its tail crossfades over 80 ms into the damaged ear (half the
  level loss in dB, the full threshold shift).
- **Trauma** (ear pressure/fatigue): only the KIA gate.

Targets cross from the simulation as atomics and ramp per sample (10 ms level, 15 ms shift); a
voice started mid-concussion begins at the current state; unimpaired playback is bit-exact.
`Audio::set_hearing(level, shift_left, shift_right)` and `Audio::set_contrast(gain)` replace the old
use of master volume as the concussion duck; master volume is again a plain output gain.

Threshold shift per damage tier (derived from the existing level floor): medium ≈0.55 (≈2.4 kHz),
heavy ≈0.86 (≈0.8 kHz), catastrophic ≈0.94 (≈0.63 kHz). It is full within the same 12 ms attack as
the level duck, holds through the stunned beat, and detail returns after level does. The ear facing
the strike takes the full shift, the far ear up to 25% less. There is still no tonal tinnitus.

Tests: `audio_ear` unit tests (bit-exact when unimpaired, contrast only on the world, fresh hit
untouched, per-ear dulling, ramped changes, per-sample KIA gate);
`hearing_loss_darkens_running_beds_and_spares_the_fresh_hit` (the real mixer: a running engine and
wind bed loses level and detail, a fresh strike is bit-exact through contrast and concussion);
`hearing_detail_returns_after_level_and_the_struck_ear_is_duller`;
`impact_bus_never_exceeds_its_ceiling_whatever_the_gain`.

## Measurements

Staged hits on a Tiger moving through a blizzard, turret laying; ordinary `Audio` sink graph at live
gains and 60 Hz control cadence; unity export (clamped as the device clamps). Baseline =
`captures/astra/impact_mix_20260928_v0_baseline_*`, after = `impact_mix_20260928_v3_final_*`.

| | Close 76 mm side penetration | 122 mm penetration | Marginal ricochet |
|---|---|---|---|
| World in the 100 ms before contact | −0.2 → **−15.3 dB** | −0.2 → **−21.7 dB** | −0.2 → **−13.6 dB** |
| First 150 ms over the world just before | −1.3 → **+21.2 dB** | +4.9 → **+36.5 dB** | +2.2 → **+14.7 dB** |
| Hit peak in the mix | 0.176 → 0.403 | 0.447 → 0.889 | 0.133 → 0.134 |
| Samples at/over full scale | 0 → 0 | 0 → 0 | 0 → 0 |
| World level 0.3–1.3 s after | −19.6 → −20.3 dB | −27.8 → −28.9 dB | no concussion |
| World share above 2 kHz, 0.3–1.3 s after | +0.2 → **−8.2 dB** | +0.2 → **−10.8 dB** | unchanged |
| …2.5–3.5 s after | 0.0 → −3.9 dB | 0.0 → −8.2 dB | |

Commander head-out (bright open-air bed), same close penetration: share above 2 kHz −10.4 dB at
0.3–1.3 s. These mono figures understate the struck ear, whose ~0.8 kHz two-pole low-pass removes
about 17 dB above 2 kHz; the less-deafened far ear dominates a mono sum.

Onset, delivered hit voice, 10 ms windows over the first 150 ms — the first 10 ms against the
loudest later window:

| | Before | After |
|---|---|---|
| Close 76 mm side penetration | 9.7 dB below (loudest at 20 ms) | **3.2 dB above** |
| 122 mm penetration | 12.4 dB below (loudest at 30 ms) | **5.4 dB above** |
| Marginal ricochet | 11.2 dB below | level (+0.2 dB) |

## Open: the player's own gun out-weighs being hit

The same fixture measures the player's own 88 mm report (the verified `gun_interior`/`gun_exterior`
recordings) as a reference: peak 0.73/0.85 and **150 ms RMS −5.0/−6.4 dBFS** — a crest factor of
about 2 dB. A close 76 mm penetration is now −22 dBFS over the same window (was −31); the 122 mm at
the ceiling is −13 dBFS. Under a −1 dBFS ceiling a transient strike cannot match that density
without becoming a sustained blast, which the approved direction rejects. This pass does not touch
the owner-verified recording. Deciding whether to lower the gun report (for example by ~6 dB) so
that being penetrated is the loudest thing in the game needs a listening decision.

## Tests

- Binary suite (test profile): **1,134 passed, 0 failed, 28 ignored**, including 13 new tests
  (ear stage, impact bus, layering, contrast, hearing, offline twin, pre-hit probe). Two
  `mechanic_capture` tests need the untracked `act1-winter-approach/20260913-v1/heights.npy`, as in
  the other checkouts.
- 23 integration crates pass. `display_quantization`, `live_fx_contract`, `physical_truth_contract`
  and `protective_glass_transport` fail exactly as on the untouched snapshot; `foot_ai_contract` and
  `combat_training` still do not compile (`crate::resource`), before and after.
- No live GPU playtest or subjective listening was run in this pass.

## Parity and other routes

- `audio_terror_review` (armor and KIA takes) and `impact_mix_review` use the live policy.
- `astra_capture` review films run the bed, strike and fragment contacts through
  `acoustics::offline_listener`, an offline twin of the listener stage using the same laws
  (test: `offline_listener_mirrors_the_runtime_buses`).
- `showreel` and other offline renders pick up the new layering through the shared synth.
- The retired tracer-landing path (`#[cfg(any())]`) is unchanged.

## Files

- `src/audio_ear.rs` (new): listener stage and buses.
- `src/audio.rs`: `create_sink_on`, `play_impact_stereo`, `set_contrast`, `set_hearing`; render-time
  concussion muffle removed (the ear applies it live).
- `src/acoustics.rs`: layered strike, blast pulse, contrast/hearing/impact laws, offline listener.
- `src/game.rs`: pre-hit probe and contrast, impact bus, hearing per ear, dismounted guard.
- `src/impact_mix_review.rs` (new): the A/B fixture above.
- `src/astra_capture.rs`, `src/audio_terror_review.rs`: parity.

## Own gun and cumulative exposure (owner decision, 28 Sep 2026)

The owner decided: "your own gun should be extremely loud ... ultimately everything should be as
loud as it would be in real life. its a simulator." The proposal to lower the own-gun recording by
about 6 dB is withdrawn; it stays the loudest routine event the crewman hears.

What full scale cannot carry, the ear does:

- Every discharge of the Tiger's own 88 now takes hearing: a brief stun (level floor 0.55 buttoned
  up, 0.40 head-out in the cupola, 1.8–2.4 s recovery) unless a stronger concussion is already
  running, plus a lasting threshold shift.
- Every strike on the hull adds to that lasting shift, penetrating or not, growing with calibre and
  nearness to the listener, most for a penetration (`acoustics::strike_exposure`).
- The shift accumulates (saturating toward deafness) and recovers with a 240 s time constant
  (`EXPOSURE_RECOVERY_S`). The ear hears the stronger of the transient and the accumulated shift,
  and a deafened ear also hears less level. Thirty rounds fired buttoned up leave the ear markedly
  duller; real temporary threshold shift after sustained gunfire lasts hours, compressed here.
- The increments and recovery time are tuning, ordered as the physics orders them (head-out worse
  than buttoned up, penetration worse than bounce, near worse than far). A calibration of every
  source to physical sound pressure level at the listener is in progress
  (`acoustics::accumulate_exposure`, `recover_exposure`, `own_gun_exposure`, `own_gun_concussion`;
  test `every_gunshot_and_hit_costs_hearing_that_builds_over_a_fight_and_recovers_over_minutes`).
