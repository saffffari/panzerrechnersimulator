# Kessel — ACOUSTICS (sound is the #1 pillar)

> Status: live shared-event implementation. Ballistics, localized armor damage, spall, concussion,
> visual FX and the parametric impact rack now consume the same resolved strike.
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
| **plate thickness** | resonator stiffness/body pitch; a mantlet and thin side plate no longer share a generic voice |
| **effective thickness + impact angle** | normal energy coupled into the plate versus a glancing, scraping strike |
| **panel** | the modal set + ring time (flat glacis booms low & long; thick mantlet rings high & short) |
| **penetrated** | bounce = full long ring (CLANNNGG); penetration = ring choked ×0.3 + a low punch-through + internal **spall rattle** (APHE adds a detonation crump) |
| **residual velocity + behind-armor energy** | punch-through weight, internal fragment density and spall-rattle count |
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
   panned to the impact bearing, loud if near your station. In the last 0.2 s before contact the
   world bed drops out, the strike's four layers land together, and a concussive hit then takes
   the listener's hearing (level and detail) — see §6.
4. **Distant gun REPORT** — the *firing* sound arrives LATER, delayed by `range / 343 m/s` (the
   speed-of-sound queue, `ScheduledSound`, already in `game.rs` — keep it). So you feel the hit before
   you hear the gun that fired it, from km out. This split is the whole effect.

## 4. Armor memory and field repair

Every player-Tiger impact now leaves one compact hull- or turret-local record. Defeats make dents;
penetrations make open holes. A later round queries the wounded point before the DeMarre solve, so a
repeat hit into a crater or hole sees reduced resistance while an untouched part of the same nominal
plate remains intact. The bounded 24-scar set persists through campaign checkpoints and LAN snapshots.

The marks are hard-edged scene geometry rather than smooth decals: dark dents/openings and three spare
track shoes fastened across a repaired breach remain attached to the correct hull or rotating turret. A dismounted player carrying the
physical toolbox can stand beside an open hole and hold the repair control for six seconds, consuming
one spare-parts unit. The resulting patch restores useful fragment/weather and light-round protection
but only about 55% of the lost resistance; it is intentionally not factory armor and can be holed again.

Interior penetration VFX now has a 48-fragment pool. The resolved behind-armor energy scales fragment
count, speed, lifetime, spall lethality and the matching synthesized rattle.

## 5. What exists vs. what's next

- **Exists:** shared `ImpactEvent`, live parametric stereo impact playback, speed-of-sound reports,
  pre-hit contrast, layered strikes on an impact bus, concussion as level and per-ear threshold
  shift (§6), localized armor scars, energy-scaled spall and campaign/LAN persistence.
- **Next:** replace the generic late tail with the CAD-derived response matrix in
  [`TANK_ACOUSTICS_PIPELINE.md`](TANK_ACOUSTICS_PIPELINE.md). Incoming rifle/MG strikes against the
  tank should use a separate capped pool of flat silver chips: visual-only, non-persistent and with
  no armor loss. The deterministic Metal replay already validates the heavy-hit attachment scale and
  the 48-fragment shower against the real Tiger skin.

## 6. The hit mix: contrast, layers, consequence

A hit feels deafening when the world drops out just before it, four layers land together, and the
player's hearing is taken away after; full scale is a hard ceiling, so level cannot do that work.
Implemented 2026-09-28, with measurements, in [`IMPACT_SOUND_20260928.md`](IMPACT_SOUND_20260928.md):

- **Contrast** — a read-only probe of the authoritative in-flight rounds predicts a strike on the
  player's armor and drops every world voice 10–26 dB over the last 0.2 s (`Audio::set_contrast`).
- **Layers** — contact, plate body, weight and steel ring all start at contact; the strike plays
  on the impact bus, untouched for 120 ms, under a −1 dBFS ceiling (`Audio::play_impact_stereo`).
- **Consequence** — every world voice passes a per-voice listener stage (`audio_ear.rs`): concussion
  level loss plus a per-ear threshold-shift low-pass, detail returning after level
  (`Audio::set_hearing`). The trauma layer stays audible; the KIA gate stays sample-exact.

Staged A/B fixture: `cargo run -- impact-mix-review <prefix>`.

## 7. Backend note

Keep rodio for playback, but do the DSP ourselves: `acoustics.rs` produces finished interleaved-stereo
buffers (pan + proximity + dulling already applied), which rodio plays as-is. This sidesteps rodio's
weak spatialisation and gives us exact control over the pillar. No new audio crate needed; the WAV
writer is hand-rolled (16-bit PCM).
