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

## 9. Binaural rendering: the head and the steel around it (`acoustics::spatial`, 2026-10-27)

Every positioned voice ends in one stage that places it around the crewman's head. The stage is
`src/acoustics_spatial.rs`: deterministic, slice-in slice-out, measured by its unit tests. It is a
parametric head model with every part named; it is **not** a measured head-related transfer
function and **not** a wave solution. No permissively licensed measured HRTF set is bundled; that
remains the upgrade path (the stage is one filter pair per voice, so a measured set would replace
it without touching the chain).

| Cue | Model | Where it comes from |
|---|---|---|
| **Direction** | Head-relative azimuth (0 ahead, 90 right, 270 left) from the station camera's forward vector: the commander's head-look, the turret the gunner and commander ride, the periscope the buttoned listener looks through. Elevation is carried but every game bearing is horizontal today. The spherical head resolves one angle, the lateral angle θ = asin(sin az·cos el): front/back and elevation along the cone of confusion are not told apart. | `game.rs` `bearing_deg_from(ear_forward, …)`; `spatial::Direction` |
| **Interaural time difference** | Woodworth's rigid sphere, radius 87.5 mm, c = 343 m/s at the head: ITD = (a/c)(θ + sin θ), 0.66 ms (29 frames) abeam, as a linearly interpolated fractional delay on the ear away from the source. | `spatial::woodworth_itd_s`, `FractionalDelay` |
| **Head shadow / ILD** | Brown & Duda's (1998) first-order head-shadow filter per ear, H(s) = (αs + β)/(s + β), β = 2c/a (624 Hz): unity at low frequency, α at high frequency, α = 2 at the ear the source faces and 0.1 at 150° from the ear's axis (the sphere's bright spot brings the antipode back to 0.28). Low frequencies pass round the head (ILD < 1.5 dB at 125 Hz); 3 kHz abeam is shadowed, not silenced. | `spatial::head_shadow_alpha`, `HeadShadowFilter` |
| **Presentation pan** | The owner's full constant-power pan (`docs/PHYSICAL_TRUTH_AUDIT.md`, owner-directed departures, 2026-10-03) stays on top of the head model, so no voice's lateralisation or level moved when the model landed. `HeadModel::PHYSICAL` renders the sphere alone (tests, tools). | `spatial::HeadModel` |
| **Distance and air** | Not here. `sound_propagation::propagate` carries the distance law (spherical, blast, rifle-blast), ISO 9613-1 air absorption from the act's air, the CRREL snow ground effect, Maekawa barrier loss, the plate and hatch insertion loss and the headset into `EarLevel.transfer_db`; `audio_calibrated::render_for_ear` applies that per-band transfer before the binaural stage and the level placement after it. | `sound_propagation.rs`, `audio_calibrated.rs` |
| **Transmission** (closed hatches, open cupola, periscope-only views) | `EarLevel.transmission` records how the sound came in: `Direct` (head-out, on foot, a source in the same compartment), `Plates` (buttoned, including every periscope view), `Hatches{open}`. The level of each way is SL §8's insertion loss; the stage decides only which direction cues survive. The plate-borne share arrives from the whole re-radiating cabin and keeps 30 % of its lateral angle [E]; the share through open hatches arrives over the rim above the head, lifted to 45° elevation [E], which shortens ITD and ILD by cos 45°. The two shares are the band-energy ratio of the buttoned and open-hatch insertion losses under the voice's spectrum at the ear, rendered separately and summed by √share. Abeam: dominant interaural lag 10 frames buttoned, 17 through one open hatch, 29 head-out. | `spatial::plate_share`, `arrival_paths`, `render_through`; `Enclosure::transmission` |
| **Continuous voices** | Other vehicles, burning wrecks (exterior: the listener's current enclosure), the manual elevation gear and the own loose track (direct) run a streaming panner: one effective direction per voice (energy-weighted mean of the two paths), parameters ramped per sample with a 20 ms constant, so a head turn or a hatch opening moves the voice without a click or a restart. | `spatial::BinauralPanner`, `audio_vehicles.rs`, `Audio::set_listener_enclosure` |

What is routed through it: cannon reports, HE bursts, passing cracks, enemy MG strikes, structure
collapse and every other scheduled world event (`render_for_ear`); strikes and MG on the own
plates (`spatialize_armor_mono`, `synth_mg_gravel`: the plate is the source, direct); hot fragment
contacts and whizzes; the owner's local recordings, controls and foley (`spatialize_mono`); the
positioned loops above. The own gun, own MG and own engine beds are not positioned (they are at
the ear by calibration).

**Proof.** `cargo run --release -- crew-battle <outdir> spatial-sweep <seed> 35` films one T-34
abeam at 100 m firing over the turret roof five times while the turret traverses through 180°,
the cupola lid opens and the commander puts his head out; the receipt's `spatial_proof` measures
the recorded gameplay audio itself (per discharge: energy against the half second before it, per
ear and above 1.5 kHz) and the run fails unless the first shot pans right, the second and third
left, the open hatch and head-out brighten, and the fifth pans right again. Unit gates:
`acoustics::spatial::tests`, `audio::vehicles::tests::exterior_vehicle_direction_follows_the_listeners_hatches`.

**Limits** (also in the ledger): a sphere, no pinna or torso (the SL §4 far-ear table is 2–5 dB
deeper than the sphere from 500 Hz to 2 kHz; agreement within 3 dB at 4 and 8 kHz); no elevation
cue beyond the lateral angle; the 30 % plate retention and 45° aperture elevation are estimates;
the hatch share uses the pool's spectrum class for loops; one speed of sound at the head; the
presentation pan is a departure, named as such.

## 10. Traced rooms: sound energy through the real geometry (`acoustics::raytrace`, 2026-10-28)

Every world voice and every foley event is now heard in a reflected field traced through the
scene, where a trace exists: the Tiger's own source triangles, the terrain, and the box cover of
buildings and wrecks. This is stochastic energy ray tracing in the geometric-acoustics
(high-frequency) limit (Krokstad, Strøm & Sørsdal 1968; Vorländer 1989), per octave band
31.5 Hz–8 kHz. It is not a wave solution and it carries no phase.

| Stage | Model |
|---|---|
| **Geometry** | The Tiger: each of the 16 articulated source frames (hull, turret, gun, lids, cupola, shutters, breech) rasterized once into a 5 cm occupancy grid in its own frame (3.56 M source triangles, ~0.6 s), dilated one cell; rays march it with a 3D DDA; normals are the smoothed 3×3×3 occupancy gradient, so slanted plates reflect along their slope. The terrain is marched and bisected, its normal from the height gradient. Cover boxes (buildings, wrecks) within 150 m are masonry. |
| **Materials** | Random-incidence absorption per band and a scattering coefficient for armour, machinery, fresh and packed snow, frozen ground, masonry, timber, rubble and wrecks (`raytrace::Material`, handbook values [E]). The cabin's frames are armour or machinery. |
| **Transport** | Rays from the source (Fibonacci lattice, seeded); air absorption per band (ISO 9613-1, the act's air); at each surface (1 − α) per band, then specular or Lambert by the scattering probability; up to 40 reflections or 0.45 s inside, 12 or 1.5 s outside. |
| **Reception** | A receiver sphere (0.25 m inside, growing with range outside) collects specular crossings; at every reflection the scattered share is sent straight to the receiver when a shadow ray sees it ("diffuse rain"), so distant listeners are not left to luck. Energy per 1 ms bin and band, relative to the direct sound, with the arrivals' lateral angle. |
| **Impulse response** | Per band, deterministic noise through a 0 dB-peak octave band-pass, scaled so its gain in the band is the traced energy of each bin; split between the ears by the arrivals' lateral angle. The voice is convolved with it by FFT (both ears in one complex transform) and added to the dry binaural path (§9). |
| **Cache** | Fields per listener cell (0.3 m in the hull's frame aboard, 4 m outside), hatch state, turret bucket (30°) and source bucket: the listener's own body, a cabin cell, the plate facing an outside source (4 quadrants), the open cupola, or an outside source by sector (8) and range class (4). A fixed ray budget per simulation update (48 rays High, 16 Laptop; 384/768 rays per field High, 128/256 Laptop) keeps captures deterministic and frames free of whole traces. Until a field is traced, the nearest traced field of its kind stands in, else the parametric tail. The cabin voxels (~0.6 s for 3.6 M source triangles) are built on a worker thread started at world load from the same surfaces `CabinFire` gets; traces wait for them, never the simulation. |

Measured (`cargo run --release -- sound-world <outdir>`, the real hull, turret, gun and cupola
meshes; receipt `artifacts/sound-world-20261028/sound_world_20261028.json`):

| Case | Reflected energy at 1 kHz | T20 | Escaped | Heard |
|---|---|---|---|---|
| Buttoned, T-34 report from the right | +8.8 dB | 0.17 s | 54 % | right ear +5.3 dB |
| Cupola open, same report | +4.0 dB | 0.22 s | 89 % | right ear +10.8 dB |
| Head out, same report | −7.4 dB | | 100 % | right ear +13.7 dB |
| On foot, brick wall 3 m to the right, report ahead | −5.9 dB | | 100 % | wall's reflections from the right |
| On foot, open snow, report ahead | −25.5 dB | | 100 % | ground reflection only |
| Loader's boots on the turret floor | +1.2 dB | 0.09 s | | |
| Boots on a timber floor, 4 × 5 m room | +10.8 dB | 0.93 s | | |
| Boots in fresh snow, open field | −8.5 dB | 0.02 s | | |

The proof loads only four of the sixteen frames, so its closed cabin has open roof-lid and
visor holes; the game's cabin has every frame. Unit gates: `acoustics::raytrace::tests` (a
shoebox decays at Eyring's rate within 20 %, a voxelized room is sealed and within 25 %, an open
hatch shortens the decay, fresh snow swallows the 4 kHz ground reflection, a wall on the right
fills the right ear, batched tracing equals one trace, the FFT convolution equals the direct
sum) and the ignored `game::game_acoustics::tests::the_real_cabin_rings_and_an_open_cupola_lets_it_out`.

**Limits.** No diffraction (the hatch rim, a wall's edge), no transmission through plates (the
hull's loss stays SL §8's, on the dry path), no modes (the cabin's below ~100 Hz are smoothed
over), no phase; early reflections are band noise per bin, not discrete image sources. Features
under 5 cm are merged or lost, every wall stands a cell thicker into the air. Trees, doors and
building interiors beyond their boxes are not in the outdoor scene. A field is traced from a
bucketed stand-in source (the plate facing the source, the cupola, a sector and range class),
not from each source's exact place. Cost bounds, each named: the reflected field is convolved
from a voice's first 0.75 s (`TRACED_WET_SOURCE_S`); at most four traced convolutions per update
on High, two on Laptop, beyond which a voice takes the parametric tail; the pre-rendered MG
receiver rounds keep the parametric path.

## 11. Foley from contact physics (`acoustics::foley`, 2026-10-28)

314 named cues, every one synthesized deterministically from a physical model and varied by an
event seed: footsteps (13 grounds × 7 gaits: walk, run, crouch, scuff, take-off, soft and hard
landings), handling (21 objects × 6 gestures: grab, set down, drop, throw, carry, stow), 17
cabin and body movements, 22 vehicle sounds, 15 weather and environment sounds, 12 cigarette
actions, 5 extinguisher sounds, 18 door sounds (timber, iron, barn) and 8 body and fuel sounds.

| Primitive | Physics |
|---|---|
| Modal strike | Inharmonic modes per body class (thick plate, sheet, tube, cast steel, brass case, timber, stone, glass, rubber, leather, canvas, bakelite, a jerry can's panels, paper, ice); a half-sine Hertzian contact pulse, t ∝ v^−1/5, so harder impacts excite higher modes. |
| Grains | Granular breakage under load: micro-fractures as short resonances in the ground's band (snow, ice, gravel, rubble, mud's suction and bubbles); dry snow's squeak below −10 °C as a stick-slip glide [E]. |
| Friction | Stick-slip events at sliding speed over a roughness length exciting the surface's modes; low rates are creaks (leather, timber, hinges, masonry). |
| Bubbles | Minnaert, f = 3.26 m·Hz / r, with the rising chirp at the surface: fuel poured from a can. |
| Flow | Nozzle hiss (the extinguisher); the Aeolian tone of the antenna, the grab rails and the open hatch rim at Strouhal f = 0.2 U / d, pitch following the wind. |
| Body | Formant bursts for a cough or an exhale; cloth rustle; a body's weight landing. |

Each cue names its level at 1 m (`Cue::ref_dba_1m`, estimates against the anchors in
`sound_levels` [E]); the game propagates it from the event's world position through
`sound_propagation` (distance, air, ground, a blocked ray, plates and hatches, headset), places
it by the binaural stage and hears it in its traced room. Continuous cues (snow and sleet on the
steel, the wind tones, the turret fan, the lamp, the tracks' ground, brakes, the hand traverse,
a wall grinding, enemy T-34 links, the extinguisher spray, fuel pouring) run as positioned loops.

Where it is heard in the game: footsteps on timber and stone floors, rubble, mud, gravel, ice and
glass (the owner's recordings keep metal, dirt, grass and snow); take-off and landing by fall
speed; the jerry can's grab, set-down, throw, bounces and the refuel (cap, pour, cap); doors
opening, closing and locked; station scrambles; every climb step's boot and grip; the hatch
passage; head out and in; casings thrown; the extinguisher taken, stowed, sprayed; the
Panzerfaust from and back to the rack; weapon switches; the toolbox set down; the torch's switch;
binoculars raised; the cigarette pack, the draw, the lighter's flint and flame, each inhale, the
cough, the ember dying and the butt flicked; clothes catching and patted out; each first-aid step
and a body dragged aside; gear changes; brakes; an empty-tank crank; fuel starvation's sputter
and stall; the hull and engine deck ticking as they cool; the MG barrel's overheat sizzle and its
cooling ticks; the tracks on snow, gravel, rubble, ice and timber; the bow stopping dead on a
wreck; a floor giving way; snow sliding off the hull; snow and sleet on the roof; the wind in the
antenna, rails and open cupola; the turret fan and lamp; wrecks cooling; enemy T-34 track links.

The own engine is now heard from the engine compartment: outside directly, inside with its
lateral angle kept by the square root of its direct share (Hopkinson–Stryker with room constant
12 m², from the traced cabin's T20), and a station level offset of a fraction of a dB from the
same law (driver 4.5 m from the engine, −0.6 dB; loader 2.7 m, −0.5 dB, against the 1.6 m
reference) [E].

Gates: `acoustics::foley::tests` (hundreds of distinct names, every cue finite, bounded,
deterministic and seed-varied, grounds and bodies in order of brightness and ring, Strouhal and
Minnaert pitches, seamless loops), `audio::foley_play::tests`, and the `sound-world` receipt's
`checks`. Not perceptually auditioned.


## 12. Tank hits from the owner's recordings (`audio::hit_recordings`, 2026-10-28)

Owner-directed: tank hits are voiced by recordings from the owner's licensed Rust & Blood SFX
Library v1.1d instead of the stock clips and the synthesized steel strike. The files are
owner-private, git-ignored under `assets/sounds`, and never committed:

| Event | Voice | Replaces |
|---|---|---|
| A tank penetrated, knocked out (ammunition or fuel) or blown apart (turret-pop) | one of eight long close explosions, `tank_hit_penetration_01..08.ogg`, never the same one twice in a row | `penetration_hit.ogg` (+ the `ap_hit_tank.ogg` layer on penetrations), the synthesized strike on the own hull, `kill_explosion.ogg`, the `turret_pop` voice |
| A strike that did not penetrate (bounce, ricochet, shatter, a burst that did not hole the plate) | one of five short close explosions, `tank_hit_nonpenetration_01..05.ogg`, high-passed by the strike's hardness | the `armor_clang` voices, `ap_hit_tank.ogg` on foot, the synthesized steel strike on the own hull |

**Hardness** (`strike_hardness`, 0..1, monotonic in each input) [E]: 0.5 × the round's energy on
a log scale from 100 kJ (a 37–45 mm round at range) to 8 MJ (a 122 mm round at the muzzle),
kinetic plus the filler's TNT-equivalent for a burst on the plate; + 0.2 × cos(obliquity); +
0.3 × penetration / effective thickness (how near it came to going through).

**Corner** (`cutoff_hz`): fourth-order Butterworth high-pass, log-interpolated from 1.5 kHz at
hardness 0 to 60 Hz at hardness 1 [E]. 60 Hz is the bottom of the 63 Hz octave, the lowest band
the hull transfer, rooms and reproduction carry: a hard strike keeps everything. 1.5 kHz is the
top of the 1 kHz octave: a glancing strike hands the plate little normal momentum (∝ m·v·cos θ)
over a short contact, so what radiates is ring and crack, not body. The A-weighted level the
filter removes is taken off the strike's calibrated level too (≤ 0 dB), so a light strike is
quieter as well as thinner. In the `hit-sounds` A/B: a 45 mm round glancing off the Tiger's glacis
at 60° rates 0.22 (744 Hz), a 76 mm round at 30° stopped 6 mm short 0.73 (141 Hz), an 85 mm
round square on the glacis 0.87 (90 Hz).

**Placement.** World hits go through the propagation queue at the event's own emission (the
strike, penetration, kill or ammunition blast levels of §8), so distance, air, ground, a blocked
ray, the listener's hull and headset, bearing and the traced room apply as to every world source.
The owner-directed presentation floor for strikes and explosions still applies after that, so at
range it can lift a light strike back to the floor. A strike on the listener's own Tiger plays at
its calibrated level at the ear (SL §5, which already falls with the round's energy, obliquity
and the station's distance) through the headset and the cabin's traced room (the parametric steel
interior until traced). The airborne plate insertion loss is not applied to it: the struck plate
is the radiator. Without the files every site keeps its previous voice; `KESSEL_HIT_RECORDINGS=0`
selects that too.

Gates: `audio::hit_recordings::tests` (hardness and corner monotonic, the high-pass at its corner
and slopes, no variant twice in a row), `game::game_hit_sounds::tests`, the `hit-sounds <outdir>`
receipt's `checks`, and the `[hit-sound]` line each live event prints. Not perceptually
auditioned beyond the A/B renders.


## 13. Handling, breech, wounds, materials and footsteps from the owner's libraries (`audio::recorded_fx`, 2026-10-28)

Owner-directed: recordings from the owner's licensed Rust & Blood SFX Library v1.1d, Echoes Audio
Super Kit v2.0.1 and ROT Horror Audio Bundle v1.8 voice the events below. The files are
owner-private, git-ignored under `assets/sounds` (`LIBRARY_20261028_SOURCES.txt` beside them maps
each to its source) and never committed. Each family's variants are used without repeating back
to back; leading silence is trimmed to 2 ms before the onset; the breech parts are resampled once
for the weight of an 88 mm round and a 400 kg block (close 0.62x, ram and extraction 0.85x [E]).
Each event plays at its family's 1 m reference level [E] plus its own offset, propagated from
where it happens through the foley path (distance, the hull between, the headset, bearing, the
traced room); world objects go through the propagation queue so their travel time stays. At most
six recorded voices start per simulation update; later requests in the same update are dropped.
Every site keeps its previous voice where a file is absent (`KESSEL_RECORDED_FX=0` too).

| Event | Recording | Replaces / adds |
|---|---|---|
| Equipment picked up (toolbox, Panzerfaust, extinguisher, jerry can, supplies, loot, shelves) | item_pickup | the pickup clip, the grab foley |
| Equipment dropped or set down (toolbox, jerry can; bounces by impact speed, set-down -6 dB) | firearm_drop 1-5 | the set-down / drop foley |
| Gear equipped or stowed (weapon switch, Panzerfaust and extinguisher stowed, flashlight, binoculars up and down, Luger drawn and holstered) | item_equip 1-4 | the grab/stow foley; lowering binoculars and the Luger draw were silent |
| A live 88 lifted from the rack, the floor or by a wreck | Echoes misc_1/misc_2 | the synthesized lift; the wreck pickup was silent |
| A live round unloaded, a case extracted (hand and crew) | Echoes slide_1 / slide_3_open, main transient at the clearing | the case-extract recording, the synthesized lift |
| The round rammed | Echoes slide_1_close / slide_2_close, last strike on the seat | the shell-ram recording |
| Lever pulled, breech closed (hand lever and the crew's close 0.33 s after the ram [E]) | Rust & Blood mag_in 1-3 | the breech-lock recording |
| Main-gun trigger pulled, no shot (no round, breech open, a staged dud primer) | pistol empty-barrel click at the gunner's hand | the failed-trigger clip; on a shot it plays 24 dB down, under the report |
| Luger chambered after an empty-gun reload | pistol_racking 1 | the toggle-pull and toggle-close clips; dry pulls now click |
| First aid: procedure begun / step done / finished or abandoned | inventory_open / medkit_use / inventory_close | the bandage-tear foley; the bag was silent |
| Map unfolded or folded, order sheet shown or folded, map sheet recovered | note_turn_page | the map clips; the sheet's showing and recovery were silent |
| Starter button, flashlight switch, intercom order keys, platoon radio key | ROT button_mx 1-4 | the lamp-switch foley; the rest were silent |
| Tank damage by component (own Tiger): vision blocks and sight glass, running gear, track, ring, mount, MG, visor; radio shorted | glass / metal impact or break; ROT electric shock | added under the strike's own voice, the strongest two, levelled by the deposited energy (0 dB at 50 kJ, +-10 dB [E]) |
| Modules wrecked by spall | metal break; electric shock for the radio | added, the first two |
| Crew wounds (one composite per man per strike, two men at most) | fragments: sticky / sharp / sharp heavy / laceration (1 in 3 over 220 J) / critical when fatal; blast: blunt bloody light / moderate / heavy; blast work over 150 J adds the body striking steel | the synthesized body impacts lost under the recorded strike |
| Bullet wounds (the man on foot, an exposed crewman) | sticky hit, critical when fatal | added |
| Bodies struck in the world (infantry) | impact_body | added under the infantry's own voice |
| Small fragments striking steel near the crew | knife_hit_hard_surface 1-3 at the struck place, by the chip's speed | the synthesized steel ding |
| Trees snapped / felled; wooden and metal scatter shoved or struck; walls struck, ground down, collapsed; a wreck's steel touched by the bow | wood break / impact; metal impact; concrete or wood impact and break by the structure's class; metal impact | the ground-impact and brass-clatter clips; a struck wall was silent; the collapse keeps its blast and adds the break |
| Footsteps of the player (on foot and in the cabin) and of infantry within 25 m (the three nearest per update; a stride of 0.75 m, 1.1 m running [E]) | Rust & Blood footsteps by surface: snow, dirt (mud and frozen earth), gravel (rubble), grass, concrete (stone floors), wood (timber floors), metal (hull deck and cabin) | the four recorded surfaces and the synthesized ones; ice and glass keep their synthesis; infantry were silent |

**Not hooked, because the event does not exist** (no gameplay was invented):
- headphones on and off, ammunition crates and wooden bins opening (the crate recordings are
  installed with nothing to call them), a dome light or panel switch, smoke dischargers;
- bodies run over, windows and bottles breaking, enemy component damage beyond engine, ammunition
  and fuel, wooden stowage on the Tiger;
- what a body struck in the cabin, beyond blast work standing for "thrown against the steel";
- 12.7-25 mm rounds (DShK, PTRD/PTRS, autocannon): no such projectiles exist, so Echoes hit 1-5
  are installed for them and unused;
- a random misfire: only the staged dud primer exists, and it now clicks.

**Skipped as non-diegetic** (CLAUDE.md's first principle and physical truth): new_objective,
player_level_up, item_craft 1/3 and item_upgrade. plastic_container_open_small was also left out
(plastic is anachronistic for 1942, and no tin or case opening exists to give it).

Gates: `audio::recorded_fx::tests`, `game::game_hit_sounds::layer_tests`, and the
`recorded-fx-reel <outdir>` renders (every family once, a hit per component class, a crush through
mixed debris). `KESSEL_RECORDED_FX_LOG=1` prints a `[recorded-fx]` line per live event. Not
perceptually auditioned beyond the renders.

## 14. The own 88's voice: diagnosis and candidates (`audio::MainGunVoice`, 2026-10-28)

Measured through the live capture graph (`main-gun-candidates <outdir>`): every shipping shot
drives the master limiter, 13.4 dB buttoned, 15.2 dB head-out and 7.0 dB for an observer at 50 m.
The calibrated short-term level puts the recordings' attacks 25-30 dB above it, past full scale,
so the limiter flattens each shot. Buttoned, the held roar plus the stacked exterior and inside
recordings keep it within 10 dB of its peak for 0.7 s in a 1.7 s cabin tail. Tinnitus is not
the cause buttoned (no threshold shift after five shots); head-out one shot arms it (37 dB
temporary shift, as SL §12 has it behind a muzzle brake).

Candidates A (the owner's recording alone), B (a heavy rifle crack an octave down, the long close
explosion low-passed at 300 Hz and cut back to a 0.22 s decay, a 34 Hz thump) and C (the Echoes
rifle crack at 0.45x, the short close explosion at 0.7x, the owner's recording below 160 Hz) are
each one source voice at the calibrated level: there are no stacked layers to sum. They take an
owner-directed -4 dB presentation trim for the firer, the hull's airborne insertion loss buttoned
(the blast is outside the plates), the cabin's or the open ground's room, and their sample peak
held at -4 dBFS. The limiter takes 0.0 dB on all nine renders. `MAIN_GUN_VOICE` in `game.rs`
selects the voice and stays `Current` until the owner picks.


**The owner's picks (2026-10-29), Tiger only** (the own gun, wingmen and allied Tigers; every
enemy gun keeps its voice). Buttoned: `Inside`, Rust & Blood sniper_rifle_heavy_fire 1/2 alternating
per shot at 0.62x over a 34 Hz thump; a third of the hull's insertion loss as its colour, the
cabin's room, the calibrated level with no trim, the peak held at -2.5 dBFS: short-term level
-23.2 dBFS, peak -2.3, limiter 0.0 dB (`mg_inside_final_buttoned.wav`). The recoil and the case
ejection stay. Head-out, a hatch open, or on foot near the tank: `Outside`, sniper_rifle_fire 5 at
0.6x with heavy_fire 1 at 0.55x below 600 Hz for body, through the open-air path with the -4 dB
firer trim (`mg_headout_final.wav`: -21.9 dBFS, limiter 0.0 dB). Observers and wingmen hear
the same voice through the propagation queue (`mg_ext50_final.wav`); beyond 700 m the owner's
distant recordings keep the far report. Buttoned or hatch-open, five shots leave no threshold
shift and no tinnitus; head-out one shot still arms it, as the ear model has it.
`TIGER_GUN_RECORDED = false` restores the shipping stack.

## 15. The T-34's report (2026-10-29)

T-34s fire with Rust & Blood old_rifle_fire 1-6 (rotated, no repeats) at 0.72x for 76.2 mm
weight, placed by the gun's propellant charge through the propagation queue: arrival delay, the
distance law, ISO 9613 air absorption, ground and snow, and buttoned the hull's insertion loss
and whole-impulse peak loss. In `recorded-fx-reel`: the 2-8 kHz band falls 18 dB more at 800 m
than at 100 m head-out; buttoned at 100 m the short-term level is 16 dB under head-out and the
2-8 kHz band 25 dB lower. Other enemy guns keep the owner's T-34 recordings and their
synthesized reports; the SU-100, KV-2 and IS series would want heavier sources than these rifle
shots (flagged, not changed). Named limit: buttoned, the A-weighted calibration of a report the
hull has made almost all sub-bass puts its unweighted peak near -1 dBFS; this is the existing
pipeline's behaviour for every exterior gun heard through the hull.

## 16. Small arms on the armour (2026-10-29)

Rifle and MG bullets striking the Tiger or any vehicle play one of the Rust & Blood suppressed-SMG
shots shaped per hit (`shape_bullet_strike`): cut to its transient, pitched +-4 %, high-passed
from 800 Hz (square) to 3 kHz (grazing), rung by three plate modes (2.9, 4.7, 7.3 kHz), with a
ricochet whine on some grazing hits and a spall tick on some square ones [E]. This replaces the
synthesized armour hit and its named light-impact character (both stay as the fallback). Inside
it plays at the SL section 5 strike level from the struck plate's bearing, dry; head-out in the open air;
outside through the propagation queue at the bullet-on-steel level. MG bursts are each round's own
contact, so the variants, pitch and filters differ round to round.

## 17. The player's body, and panning (2026-10-29)

When the player himself is wounded (a bullet on foot, a strike that wounds the local crewman),
impact_body 1/2 alternate as a low-end layer at his own head under the wound layer; crew wounds
keep the Melee set alone, and bodies in the world keep impact_body (section 13). Every recording
here is placed at its source and rendered through the binaural stage by its bearing from the head:
hull hits from the struck plate, wounds from the wounded man, the breech and the round from the
breech and the rack slot, switches and kit from the hands, footsteps under each walker, guns from
their muzzles. Checked in the renders: strikes on the left plate come out 28-49 dB louder in the
left channel, and with the constant-power presentation pan a dry hit hard left is in the left
channel only.

## 18. Rounds and cases on the cabin floor (branch no-cabin-rattle, 2026-10-29)

Owner: only a drop sounds. A loose round or case voices exactly its first audible landing after it
is ejected, released, tossed, heaved out or knocked loose, at the level its impact speed gives;
later bounces, slides and the hull's rocking are silent.
