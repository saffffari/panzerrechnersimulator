# Kessel — ACOUSTICS (sound is the #1 pillar)

> Status: design + first slice implemented (`src/acoustics.rs`: parametric impact synth + WAV render).
> Sound carries ~85% of the game's emotional impact (kessel-sound-pillar memory). Tommy scores; the
> user sources/makes SFX; Claude may use the PSX pack placeholders and download sounds as needed.
> The bar is **spectacular, not accurate** — parametric is fine.

## 1. The thesis: one impact event, seen and heard

Every round that strikes the tank is ONE event with parameters — shell mass, velocity, type, the hull
panel struck, penetrate/bounce, residual energy, the bearing to your ear, and how close it landed to
the manned crew station. That **single `ImpactEvent` drives the visual FX AND the parametric sound**
(and, later, the damage model). This is the same "shared event" that unifies ballistics and audio —
build it once, in the ballistics/damage resolution, consume it three ways.

The player never hears a number. What they hear is a CLANG that *sounds like what hit them and where*:
a light 76 mm off the glacis rings different from a KV-2's 152 mm; a hit on the left turret comes from
the left and is LOUD because they're right there; a penetration chokes the ring and punches through.

## 2. Parametric impact synth (`src/acoustics.rs`, implemented)

A struck-resonator model — the hull panel is a metal body with a few **inharmonic modal frequencies**;
the shell excites it with energy ≈ ½mv². Deterministic (counter-hash noise, no `rand`) but varied per
hit via an event seed, so no two impacts are identical. Mapping:

| Parameter | Drives |
|---|---|
| **calibre** | body pitch — big shell → lower, boomier fundamental (`(80/cal)^0.35`); tilts energy to the low mode |
| **½mv² (energy)** | excitation loudness (√E, normalised) |
| **velocity** | brightness — fast round → sharper attack transient + louder upper partials |
| **panel** | the modal set + ring time (flat glacis booms low & long; thick mantlet rings high & short) |
| **penetrated** | bounce = full long ring (CLANNNGG); penetration = ring choked ×0.3 + a low punch-through + internal **spall rattle** (APHE adds a detonation crump) |
| **bearing** | constant-power **stereo pan** (left hit → left ear) + rear dulling (behind = HF rolloff) |
| **station_dist** | proximity gain — near the crew = loud & full; far compartment = quiet & dull |

MG fire = `synth_mg_gravel`: a burst of many tiny stochastic ticks (noise + a pitched ping) — "light
gravel tossed against the hull," panned to the side it rakes.

**Verification** (audio's headless-screenshot equivalent): `cargo run -- sfx <prefix>` renders demo
impacts to `<prefix>_<name>.wav` and prints duration + L/R energy. Unit tests assert the model:
left-hit → left channel dominates, penetration rings shorter than bounce, bigger calibre rings lower
(zero-crossing proxy), output is finite & within [-1,1].

## 3. The flash → arc → CLANG timeline (the centerpiece)

The user's marquee moment, in ordered events:

1. **Muzzle flash** — seen instantly (light travels ~free at these ranges).
2. **Shell travel** — the round ARCS across the map over real flight time (ties to the ballistics
   engine: muzzle velocity, drag, gravity). You watch it come.
3. **Impact CLANG** — **local, immediate** on your hull the instant it lands: the parametric §2 synth,
   panned to the impact bearing, loud if near your station.
4. **Distant gun REPORT** — the *firing* sound arrives LATER, delayed by `range / 343 m/s` (the
   speed-of-sound queue, `ScheduledSound`, already in `game.rs` — keep it). So you feel the hit before
   you hear the gun that fired it, from km out. This split is the whole effect.

## 4. What exists vs. what's next

- **Exists:** the speed-of-sound delay queue (`game.rs` `ScheduledSound`), seamless-loop ambience +
  engine-pitch (rodio), and now the parametric impact synth (`acoustics.rs`, offline WAV render).
- **Next:** (a) define the shared `ImpactEvent` in the ballistics/damage path; (b) build `ImpactParams`
  from it and route the synthesized buffer through the audio backend; (c) real-time stereo/positional
  playback — rodio plays mono-summed, so we feed it the pre-spatialised stereo buffer we already build
  here (full control), rather than fighting rodio's limited spatial sink; (d) enclosure reverb
  (buttoned hull vs open vs urban canyon) + distance HF rolloff on the report; (e) wire MG gravel + the
  penetration spall to live hits.

## 5. Backend note

Keep rodio for playback, but do the DSP ourselves: `acoustics.rs` produces finished interleaved-stereo
buffers (pan + proximity + dulling already applied), which rodio plays as-is. This sidesteps rodio's
weak spatialisation and gives us exact control over the pillar. No new audio crate needed; the WAV
writer is hand-rolled (16-bit PCM).
