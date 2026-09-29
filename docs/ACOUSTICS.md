# Kessel — ACOUSTICS (sound is the #1 pillar)

> Status: live shared-event implementation. Ballistics, localized armor damage, spall, concussion,
> visual FX and the parametric impact rack now consume the same resolved strike.
> Sound carries ~85% of the game's emotional impact (kessel-sound-pillar memory). Tommy scores; the
> user sources/makes SFX; Claude may use the PSX pack placeholders and download sounds as needed.
> **The owner's rule (28 Sep 2026): "your own gun should be extremely loud. i mean ultimately
> everything should be as loud as it would be in real life. its a simulator."** Every source is
> calibrated in physical units, propagated to the listener's ear and heard through the listener's
> ears; one named reproduction transform turns ear levels into samples (§8). Parametric synthesis
> is fine for a waveform; its level is never authored.

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
| **½mv² (energy)** | excitation strength of the waveform (√E); the level heard is set by calibration: SL §5's ringing-energy law at the ear (`sound_levels::own_armour_strike`), the voice normalised to it |
| **velocity** | brightness — fast round → sharper attack transient + louder upper partials |
| **plate thickness** | resonator stiffness/body pitch; a mantlet and thin side plate no longer share a generic voice |
| **effective thickness + impact angle** | normal energy coupled into the plate versus a glancing, scraping strike |
| **panel** | the modal set + ring time (flat glacis booms low & long; thick mantlet rings high & short) |
| **penetrated** | bounce = full long ring (CLANNNGG); penetration = ring choked ×0.3 + a low punch-through + internal **spall rattle** (APHE adds a detonation crump) |
| **residual velocity + behind-armor energy** | punch-through weight, internal fragment density and spall-rattle count |
| **bearing** | constant-power **stereo pan** (left hit → left ear) + rear dulling (behind = HF rolloff) |
| **station_dist** | tone (near the crew = full; far compartment = dull); its level at the ear comes from the same calibration (+3 dB within 1 m of the plate, −12 dB from another compartment) |

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
3. **Crack** — a round that passes close is heard first by its conical shock (the Whitham N-wave at
   the miss distance), from the point up-range where that shock left the trajectory, a few ms after
   the closest approach. A firer behind his own muzzle never hears it.
4. **Impact CLANG** — **local, immediate** on your hull the instant it lands: the parametric §2 synth
   at the strike's physical level at the ear (SL §5), panned to the impact bearing. There is no
   warning before it (the former pre-hit drop was removed by the owner: a supersonic round gives
   none); the strike's layers land together and its pressure at the ears takes hearing afterward
   (§6).
5. **Distant gun REPORT** — the *firing* sound arrives LATER, delayed by `range / c` with the speed
   of sound from the act's air (319 m/s at −20 °C), through the queue in `game.rs`
   (`ScheduledSound`). So you feel the hit before you hear the gun that fired it. Over snow the
   report arrives as a low thump: the snow's ground effect takes the upper bands (§8).

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

- **Exists:** shared `ImpactEvent`, live parametric stereo impact playback at physical levels,
  speed-of-sound reports from the act's air, per-round Whitham cracks, layered strikes on an impact
  bus, the ear model (threshold shift, recruitment, reflex, rupture, tinnitus; §6), localized armor
  scars, energy-scaled spall and campaign/LAN persistence.
- **Next:** replace the generic late tail with the CAD-derived response matrix in
  [`TANK_ACOUSTICS_PIPELINE.md`](TANK_ACOUSTICS_PIPELINE.md). Incoming rifle/MG strikes against the
  tank should use a separate capped pool of flat silver chips: visual-only, non-persistent and with
  no armor loss. The deterministic Metal replay already validates the heavy-hit attachment scale and
  the 48-fragment shower against the real Tiger skin.

## 6. The hit and the ears

A strike's layers land together, at the strike's physical level at the ear; the listener's hearing
is taken by the pressure that reached each ear, not by an authored envelope. History and
measurements of the former authored mix are in [`IMPACT_SOUND_20260928.md`](IMPACT_SOUND_20260928.md).

- **No warning** — the pre-hit "world drops out" contrast was removed on 28 Sep 2026 (owner
  decision): a supersonic round gives no acoustic warning.
- **Layers** — contact, plate body, weight and steel ring all start at contact; the strike plays on
  the impact bus, untouched for 120 ms, then follows the ears (`Audio::play_stereo_at_ear`).
- **The ears** (`hearing.rs`, SL §12 runtime fallback) — every impulse (own gun, each MG round, the
  P08, strikes on the own hull, reports, bursts, cracks) reaches each ear as an incident peak;
  temporary threshold shift accumulates on a 4 dB-per-decade trade (TTS2 = 2·(X − 157) at 4 kHz),
  recovers in log time over 16–72 h (real time by default), leaves a residue after acoustic
  trauma; the engine accumulates slowly; eardrums rupture by the report's probit; tinnitus follows
  the report's onset rule.
- **The middle-ear reflex** (`hearing.rs`, SL §9a) — driven by the short-term level at the ear
  (each impulse, and the running engine), it contracts after a 25 ms dead time with a 65 ms
  (impulses) or 113 ms (sustained sound) time constant up to 15 dB below 0.7 kHz, relaxes on the
  same constants once the sound has gone, and adapts under a sustained sound near its threshold
  while holding in an intense one. It cannot protect against a shot's own first moments; it does
  protect impulses that follow faster than 2–3 per second; a moving Tiger's bed holds it
  contracted.
- **What is heard** — every voice passes the listener stage (`audio_ear.rs`): five-band loudness
  recruitment (soft sounds lose the shift, loud sounds keep their level), the reflex on its own
  course between updates, and a ruptured ear's conductive loss. Tinnitus is a quiet
  one-third-octave band of noise, never a sine, on the trauma bus; the KIA gate stays
  sample-exact.

Calibration check: `cargo run --release -- impact-mix-review <prefix>` renders every key event
through the live graph, measures it (LAF,max and peak, dBFS) and asserts the dB relations;
`tools/audio_levels.py` measures any capture the same way.

## 7. Backend note

Keep rodio for playback, but do the DSP ourselves: `acoustics.rs` produces finished interleaved-stereo
buffers (pan + proximity + dulling already applied), which rodio plays as-is. This sidesteps rodio's
weak spatialisation and gives us exact control over the pillar. No new audio crate needed; the WAV
writer is hand-rolled (16-bit PCM).

## 8. Levels: from the source to the samples

- **Sources** (`sound_levels.rs`): one entry per source in physical units — Z-peak and short-term
  A-weighted level LAF at a stated reference, spectrum class, distance law, durations, provenance.
  The own 88 at the ear: 163 dB / 139 dB(A) buttoned, 168 / 142 hatch open, 182 / 152 head-out.
- **Propagation** (`sound_propagation.rs`): Kingery-Bulmash blast, spherical spreading, muzzle
  directivity, ISO 9613-1 air absorption from the act's air, the snow's ground effect (CRREL 93-20),
  barrier loss when the ray is blocked, the hull's insertion loss and the crew headset.
- **Reproduction transform (not physics)** (`reproduction.rs`, `audio_master.rs`): the SL §11 tone
  map of each voice's ear LAF (1:1 to 110 dB(A), 2:1 to 150, 4:1 above) with one global exposure
  that puts the verified buttoned own-gun recording's peak at −1.5 dBFS; each voice normalised by
  its own measured LAF,max; a −1 dBFS true-peak limiter (200 ms release) on the master. The running
  bed sits ~15 dB (LAF) below the own gun.

The live debt ledger for all of this is `docs/PHYSICAL_TRUTH_AUDIT.md` ("Audio (sound levels)").
