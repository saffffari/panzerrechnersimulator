# Panzerrechnersystem: what is not simulated, or only partly — audit of the live code

- **Date:** 2026-09-28. This was a read-only audit: no repository file was changed and nothing was run.
- **Code audited:** worktree `sound-overhaul-20260928`, branch `claude/sound-overhaul-20260928`. The tree was clean at commit `351c6f0`, and every file:line reference is to that commit.
- **How it was done:**
  - I audited sound myself.
  - Six parallel read-only sub-audits covered the other areas: vehicle dynamics; ballistics and armour; damage and logistics; crew, optics and radio; weather, terrain and destruction; infantry, AI and Soviet weapons.
  - I re-opened and confirmed the citations each conclusion rests on; they are listed in §17.
  - Vision documents were used only to contrast with the code, never as evidence that something is implemented.

Conventions:
- **Inferred:** marks a claim derived from code paths, geometry or arithmetic, not something the code states or computes directly.
- Historical figures are general-knowledge orientation values, not verified in this audit. Under CLAUDE.md they need recorded sources before they become constants.
- Effort estimates are rough:
  - **S**: a contained change, days.
  - **M**: a new model plus integration, 1–3 weeks.
  - **L**: a new subsystem or a large content lift, a month or more.

---

## Headline

1. **Sound has no physical unit anywhere.**
   - Every level is a mix gain chosen for readability.
   - Distance falls off as 180/(180+d), about 16 dB from 10 m to 1 km, against 40 dB for spherical spreading alone.
   - A closed hull removes about 2 dB.
   - The engine is capped "fatigue-safe".
   - No enemy vehicle makes any engine or track sound.
   - Your own 88 is the densest sound in the game: about 16 dB (150 ms RMS) above a close 76 mm penetration.
2. **Every tank, Tiger or Soviet, moves by the same speed-chase:** 0 to 43 km/h in 2 s, 40°/s turns, reverse as fast as forward. The hull never pitches or rolls, and there is no mass, power, gear, grip, snow or slope.
3. **The lone crewman dies from any fragment of 0.24 J or more.** Fragments are capped at 220 m/s.
   - Head-out, he cannot be hit by anything outside the tank.
   - He switches stations instantly and can drive from the gunner's sight.
4. **Enemy tanks die on any penetration.** Whether a wreck's turret pops or it cooks off is a roll keyed to a global kill counter.
5. **Armour and gunnery gaps:**
   - Roof and belly contacts are discarded.
   - The mantlet is almost never the plate struck, and the 60 mm lower hull side does not exist.
   - Every shell is deleted at 1,500 m.
   - The gunner's sight has no range setting.
6. **The winter has no temperature.** Night is 20 % of the Sun's irradiance, with the light source 35° above the horizon.
7. **The weapons that actually hurt Tigers in 1942–43 are missing:** anti-tank rifles, mines, artillery and mortars, the ZiS-3. Meanwhile Act 1 ships six Panzerfausts, a 1944 weapon.
8. **The debt ledger (`docs/PHYSICAL_TRUTH_AUDIT.md`) names almost none of this.** Its armour P0 row is stale.

---

## 1. Sound propagation and levels

### 1.1 Levels are mix gains, not physical quantities
- **(a) What the code does**
  - The code says so itself:
    - Impact voices are "authored modal estimates, not measured Tiger eigenmodes or calibrated acoustic pressure" (acoustics.rs:7-11).
    - The pressure layer is "mix weighting, not a pressure-to-Pa calibration" (acoustics.rs:1422).
    - The ear is "a mix/listener model, not an auditory-physiology simulation" (audio_ear.rs:13-14).
    - No audio module contains a dB SPL value, a pascal, or a reference distance.
  - Combat gains (game.rs:601-615):

    | Constant | Gain |
    |---|---|
    | `VOL_ENEMY_GUN` | 1.2 |
    | `VOL_IMPACT_TANK` | 1.15 |
    | `VOL_CLANG` | 1.25 |
    | `VOL_NEAR_MISS` | 1.25 |
    | `VOL_PENETRATION` | 1.2 |
    | `VOL_HIT_ON_PLAYER` | 2.2 |
    | `VOL_KILL` | 1.5 |
    | `VOL_AMBIENT_EXP` | 1.5 |

    They are "nudged above unity so incoming/outgoing fire reads CLEARLY against the deliberately near-silent engine+wind mix" (game.rs:602-604).
  - Engine gain is 0.18 at idle and 0.56 at full load, commented "fatigue-safe beneath combat transients" (audio.rs:176-177). It is the same inside and outside the tank (`engine_mix_targets`, audio.rs:1369-1379, takes no interior input).
  - The weather bed is soft-bounded below 0.30 peak (weather_audio.rs:153-155).
  - MG loop is 0.85 inside / 0.70 outside. Turret traverse is 0.22 / 0.095 (audio.rs:199-208).
  - Impact loudness is √(kinetic energy) × incidence (acoustics.rs:585) × proximity 1.6/(1.6+d) (acoustics.rs:625). It is energy-motivated but has no unit.
  - Every synthesized distant gun report is re-normalised to 0.85 peak (acoustics.rs:2035).
  - Policy doc: "The bar is spectacular, not accurate — parametric is fine" (docs/ACOUSTICS.md:7). This contradicts the owner's 2026-09-28 rule.
- **(b) What is missing:** a calibration table in physical units. Relative loudness between sources is authored, not computed.
- **(c) Why it matters:** inside a running, buttoned-up Tiger the Maybach and tracks were the dominant sound, and distant fire arrived as thuds through steel. Head-out, the commander heard the battlefield directly and took his own gun's blast.
- **(d) Effort:** M for a calibration table plus migrating existing sources (see §1.8).

### 1.2 Distance, air, ground and weather
- **(a) What the code does**
  - **Distance law:** amplitude falls as 180/(180+d) (`SOUND_REF_DIST`, game.rs:601; used at game.rs:21527, 23818, 24081). *Inferred:* going from 10 m to 1 km costs 15.9 dB, against 40 dB for spherical spreading alone.
  - **Air absorption** exists only inside the synthesized distant gun report, as a one-pole low-pass at 6 kHz·e^(−r/900 m) (acoustics.rs:2016-2018). The buffer is then re-normalised (acoustics.rs:2035), so range changes timbre but not level there. Sampled world one-shots get no air absorption.
  - **Speed of sound** is fixed at 343 m/s, the value for about 20 °C (pressure_front.rs:8; game.rs:600; crew_physics.rs:17). *Inferred:* at −25 °C it is about 316 m/s, so every arrival is about 8 % early (≈0.25 s per km).
  - **Missing effects:** no ground effect over snow; no wind or temperature-gradient refraction; no humidity. Nothing in acoustics.rs or audio.rs reads wind direction, temperature or humidity. Wind speed only drives the wind-noise bed (audio.rs:2107-2124).
  - **Doppler:** none from moving sources (the outgoing "shriek" in §1.6 is scripted).
  - **Spatialisation:** constant-power pan plus a head-shadow filter (0.62 ms interaural delay, far ear low-passed to 1.85 kHz and −3.9 dB) (acoustics.rs:2211-2251). No HRTF and no elevation cues.
- **(b) What is missing:**
  - dB spreading from the source;
  - per-band air absorption (ISO 9613-1) computed from temperature and humidity;
  - ground effect with a snow surface impedance;
  - diffraction over terrain and buildings;
  - refraction by wind and temperature (downwind carry, upwind shadow zones, night inversions over snow);
  - speed of sound as a function of temperature;
  - Doppler.
- **(c) Why it matters:** winter steppe acoustics are extreme. Fresh snow deadens nearby sound, while still, cold nights carry engines and guns for kilometres. Wind direction decided who heard whom (general knowledge; source before tuning).
- **(d) Effort:** M.

### 1.3 Obstruction and the hull
- **(a) What the code does**
  - Obstruction is a single binary line-of-sight test: blocked gives occlusion 0.78, and a buttoned hull adds 0.34 (game.rs:15275-15283).
  - `apply_occlusion` converts that to a gain of 1 − 0.62·o plus a low-pass (acoustics.rs:2193-2208). *Inferred:* a building between you and a gun costs about 5.7 dB; being buttoned up costs about 2 dB.
  - `interior_muffle` is a one-pole low-pass with **no level loss** (about 2.3 kHz at the 0.92 used when buttoned) (acoustics.rs:844-860; applied at game.rs:15294-15296 and 15305).
  - Reverb is one of three Schroeder presets, picked by counting box occluders within 85 m (game.rs:15245-15256; acoustics.rs:2054-2073).
- **(b) What is missing:**
  - frequency-dependent transmission loss through 60–100 mm steel, including the real leak paths (vision slits, hatch seals, ventilator, mantlet gap);
  - hatch states beyond the commander's and driver's hatches;
  - structure-borne paths for hits and for the engine;
  - the passive attenuation of the crew headset;
  - masking by your own engine.
- **(c) Why it matters:** *inferred:* mass-law transmission loss for 100 mm steel is about 50 dB at 100 Hz. Apertures lower the real insertion loss a great deal, but it remains tens of dB, not 2.
- **(d) Effort:** M.

### 1.4 Sources that do not exist
- **(a) What the code does**
  - Only the player's Maybach has an engine voice (audio.rs:173-181, 2138-2188). Its RPM and gear are guessed from road speed and stick inputs (audio.rs:228-276; game.rs:15160-15200).
  - No enemy engine or track sound exists anywhere in audio.rs, acoustics.rs or game.rs.
  - The Tiger has no separate track or running-gear voice.
  - Enemy tank MGs reuse the rifle voice (audio.rs:335-337).
  - Artillery, mortars, aircraft and anti-tank-rifle strikes are not simulated, so they make no sound (§9).
- **(c) Why it matters:** the T-34's diesel and its track squeal were a Tiger crew's early warning in a blizzard or at night. The AI needs the same cue in reverse (§8.2).
- **(d) Effort:** M.

### 1.5 The ear
- **(a) What the code does**
  - `audio_ear.rs` is a per-voice listener stage with three buses: world, impact and trauma.
  - Threshold shift is an authored low-pass sweeping from 16 kHz down to 520 Hz (acoustics.rs:1826-1827, 1899-1903). A concussion ducks the level (acoustics.rs:1740-1756).
  - Cumulative exposure:
    - own gun: +0.05 per shot buttoned, +0.09 head-out (acoustics.rs:1957-1960);
    - hits on your own hull, scaled by calibre and distance (acoustics.rs:1972-1976);
    - recovery time constant 240 s, "compressed so a mission stays playable" (acoustics.rs:1936-1940).
  - Those are the only two places exposure is added (game.rs:22488, 22790).
  - There is no tinnitus tone, by design (acoustics.rs:862-863).
  - Exposure resets every match (game.rs:20023). *Inferred:* it is not saved.
- **(b) What is missing**
  - These cost no hearing at all: MG34 fire inside the turret, nearby HE while head-out, the pistol, hours of engine noise.
  - No acoustic reflex, permanent threshold shift, eardrum rupture or balance disturbance.
  - The crew-physics code already computes confined-blast energy (crew_physics.rs:424-440), but nothing feeds it to the ear.
- **(c) Why it matters:** hearing loss among tank crews was routine. A deafened commander who misses a T-34's engine is a consequence that emerges naturally once levels are physical.
- **(d) Effort:** M–L.

### 1.6 Non-physical cues in the mix
- **(a) What the code does**
  - **Outgoing "shell shriek":** it bends down in pitch as it recedes and plays only head-out (audio.rs:183-191; game.rs:22818-22820). *Inferred:* someone behind a Mach 2+ round hears the muzzle blast, not a receding shriek.
  - **Near-miss crack:** one clip at a fixed volume whenever a Soviet round passes within 9 m and then lands in terrain or cover (game.rs:473, 22208-22216, 22297-22303). It is not derived from miss distance, Mach number or calibre.
  - **Ambient battlefield explosions:** random booms every 6–20 s at 380–850 m with no simulated source (game.rs:616-622, 15312-15328).
  - **Pre-hit world drop:** the world goes 10–26 dB quieter before an incoming hit lands (acoustics.rs:1785-1824). It is a named presentation choice (PHYSICAL_TRUTH_AUDIT.md:24-31) with no physical counterpart. It now sits in tension with the loudness rule, so keeping it is an owner decision.
- **(b) Why this conflicts:** CLAUDE.md principle 2 rules 3–5 require such shortcuts to be gated to debug only, or named. The ambient booms manufacture events that never happened.
- **(d) Effort:** S to gate or remove. M to replace the booms with simulated off-map fire (§9.3).

### 1.7 How loud the player's own 88 is relative to everything else
The own gun is the owner-verified `gun_interior`/`gun_exterior` recording pair (audio_feedback.rs:195-203), played at unity gain (game.rs:22803-22812; audio.rs:1821-1836). The only thing that changes it is listening position: head-out means the commander is outside or the driver's hatch is open (game.rs:22786-22787). The figures below are the live-gain measurements from docs/IMPACT_SOUND_20260928.md:128-159, plus arithmetic from the constants above.

| Event | How its level is set | Level in the mix |
|---|---|---|
| Own 88, buttoned up | recording at unity | 150 ms RMS **−5.0 dBFS**, peak 0.73 |
| Own 88, head-out | recording at unity | 150 ms RMS **−6.4 dBFS**, peak 0.85 |
| 122 mm penetration beside the crew station | synth × 2.2, capped at −1 dBFS (acoustics.rs:1767) | −13 dBFS |
| Close 76 mm penetration | same path | −22 dBFS |
| Enemy 76 mm report at 100 m / 1 km | 0.85 peak × 1.2 × 180/(180+d) | peak ≈0.66 / ≈0.16 (*inferred*) |
| Engine at full load | loop × 0.56 | gain only; no absolute level measured |
| Blizzard | soft-bounded | ≤0.30 peak |

- **(b) Today:** your own gun outweighs everything, including a penetration beside the crewman. The impact pass left one question open, whether to lower the gun by about 6 dB. The owner then ruled: "own gun extremely loud; everything as loud as real life."
- **(c) What is physically right** (*inferred* from physics; I know of no Tiger measurements):
  - **Head-out:** the 88's double-baffle muzzle brake throws blast back and sideways toward the cupola. The own gun should be by far the loudest routine event.
  - **Buttoned up:** most of the blast is outside the armour. Inside, the crew hears the breech, the recoil and pressure coming through the mantlet: very loud, but hull-attenuated.
  - **A penetrating APHE whose filler bursts inside the fighting compartment** is a confined detonation, and should exceed the buttoned-up gun. A heavy non-penetrating hit next to the head plausibly rivals it.
  - So "own gun always loudest" is correct head-out and wrong for an interior burst. The physical rule is **each event at its computed pressure at the ear**, not a fixed ranking.

### 1.8 What "as loud as real life" requires
1. **Calibrate sources in physical units (S–M).**
   - Each emitter declares peak SPL (dB re 20 µPa) and an energy spectrum (SEL or sound power), at a reference distance and direction, with provenance recorded in its metadata.
   - Recordings are normalised to that calibration, not to their own peaks.
   - Orientation values from the general literature, unverified here:
     - a rifle at the shooter's ear is about 155–165 dB peak;
     - large guns near the muzzle exceed 180 dB;
     - modern tracked-vehicle interiors measure roughly 100–120 dB(A) at speed;
     - eardrum rupture begins around 35 kPa (≈185 dB).
2. **Propagate in dB (M).**
   - spherical spreading from the source;
   - per-band absorption (ISO 9613-1) from temperature and humidity;
   - ground effect with a snow surface impedance;
   - path-difference diffraction, using the existing line-of-sight geometry;
   - wind and temperature refraction;
   - speed of sound varying with temperature;
   - Doppler.
   The same result must feed the AI's hearing (§8.2).
3. **Treat the Tiger as an acoustic enclosure (M).**
   - Transmission loss per hatch, visor and vent state.
   - Structure-borne paths.
   - Headset attenuation and the intercom.
   - Engine and tracks at their true level as the masker, which means removing the "fatigue-safe" cap.
4. **Drive the ear from the pressure at each ear (M–L)** instead of authored severity tiers.
   - A hazard model such as the AHAAH or LIAeq100ms methods of MIL-STD-1474E.
   - The acoustic reflex, which acts after a latency, so it protects against sustained noise but not the first impulse.
   - Temporary threshold shift that grows and recovers on physiological time constants; permanent shift; tinnitus.
   - Eardrum rupture and balance disturbance, coupled to crew damage.
5. **One fixed output exposure (S, once steps 1–4 exist).**
   - A single mapping from ear pressure to dBFS: the audio counterpart of the renderer's fixed camera exposure (renderer.rs:750-754).
   - Keep the hard −1 dBFS ceiling.
   - Whatever full scale cannot reproduce is carried by the ear model's consequences, never by turning other sounds up. The game industry calls this "HDR audio"; here it would be physically anchored.
6. **Gate or remove the non-physical cues (S).** Derive the near-miss crack from the shell's shock wave (miss distance, Mach number, calibre).
7. **Add the missing sources (M):** enemy engines and tracks, your own tracks, artillery and mortars.
8. **Validate (S).** Extend `impact-mix-review` into a calibration test that asserts the dB differences between sources within tolerance.

*Inferred consequence:* an honest mix inside a running, buttoned-up Tiger is dominated by the tank itself, and most distant combat becomes faint thuds. That is historically why commanders rode head-out, and it makes the engine-off "silent watch" a real listening tactic.

---

## 2. Vehicle dynamics

### 2.1 The motion model
- **(a) What the code does**
  - `Tank::update` (tank.rs:850-871) steps speed toward drive × TOP_SPEED by at most ACCEL·dt (tank.rs:858-861). Heading changes by steer × TURN_RATE × dt (tank.rs:862). Height is the terrain height at the hull centre (tank.rs:864).
  - Constants: TOP_SPEED 12 m/s (43 km/h), ACCEL 6 m/s², TURN_RATE 0.7 rad/s (tank.rs:27-29).
  - The hull pose is translation × yaw only (tank.rs:899-901).
  - The same function drives the player (game.rs:13921), every Soviet type (game.rs:23429) and the ally Tigers (game.rs:13451).
  - It runs on the variable frame time with no substeps, whereas projectiles use a fixed 240 Hz step (game.rs:2604).
- **(b) What is missing:** mass, force, power and traction. Reverse is as fast as forward: S gives drive −1 (main.rs:9019).
- **(c) Why it matters**
  - *Inferred:* holding 6 m/s² at 12 m/s needs about 4.1 MW, roughly 8.6× the HL 210's ~478 kW.
  - The repo's own spec estimates 25–35 s to top speed and about 10–12 km/h in reverse (TIGER_I_SPEC.md:15, 17).
- **(d) Effort:** M.

### 2.2 Engine, gears and RPM
- **(a) What the code does**
  - The simulation has no engine power, torque, RPM or gear.
  - An 8-forward/4-reverse gear and an RPM value are guessed for the sound mixer only, from road speed (audio.rs:228-276). The engine "load" comes from stick inputs (game.rs:15160-15200).
  - Mass is used only when separating colliding tanks (game.rs:25370, 7273-7282).
- **(b) What is missing:**
  - Olvar gear ratios (the spec's gear speeds are at TIGER_I_SPEC.md:17);
  - tractive force limited by engine power;
  - one RPM value shared by sound, fuel and turret traverse.
- **(c) Why it matters:** at about 11–12 hp/t (the T-34 had about 18) the Tiger was ponderous. Escape timing, noise and fuel all hinge on that.
- **(d) Effort:** M.

### 2.3 Steering and brakes (Henschel L 600 C)
- **(a) What the code does**
  - The turn rate is a constant 0.7 rad/s at any speed; a pivot on the spot takes 9 s.
  - There are no brakes and nothing holds the tank on a slope.
  - The driver's steering wheel is animation only (driver_controls.rs:24-38).
  - *Inferred:* full lock at 12 m/s gives a 17 m radius and 0.86 g sideways, with no sliding.
- **(b) What is missing:** a fixed turn radius per gear (commonly cited as about 3.4 m in 1st to about 165 m in 8th; source before use); tighter turns made by braking one track, with skid and loss of speed; pivoting only in neutral.
- **(d) Effort:** M.

### 2.4 Grip, surface, snow and mud
- **(a) What the code does**
  - `Tank::update` takes no surface input at all.
  - Surface zones (mud, snow cover) exist in environment_authoring.rs:401, 435-515, but only the editor reads them (editor.rs:772, 1097).
  - Track ruts are drawn for rendering only (game.rs:14252-14268; renderer.rs:5548-5562).
- **(b) What is missing:**
  - rolling resistance per surface;
  - sinking according to ground pressure (commonly cited: Tiger about 1.04 kg/cm², T-34 about 0.7);
  - bellying on snow deeper than the ~0.47 m clearance;
  - a grip limit;
  - sliding on ice.
- **(c) Why it matters:** *inferred:* with about 75 % drivetrain efficiency and 10–20 % rolling resistance in snow, the sustainable speed is about 11–23 km/h. The game allows 43 km/h on any ground.
- **(d) Effort:** M for the model; L together with a snow-depth layer (§7.6).

### 2.5 Slopes, hull attitude, suspension and rollover
- **(a) What the code does**
  - Height is sampled at the hull centre only, with no pitch or roll. The test comment at tank.rs:1126 says "Terrain currently translates an upright hull."
  - There is no slope force: any slope is climbed at full speed.
  - Act I collision terrain has 4.29 m spacing.
  - `Exterior::roughness()` (tiger_damage.rs:412-415) is never called outside a test.
- **(b) What is missing:**
  - sampling along both track lines, with spring-damper suspension;
  - gradient, step and trench limits (the spec cites about 0.79 m and 2.3 m);
  - sliding and rolling over on side slopes;
  - gun cant on uneven ground;
  - loss of gun depression when parked nose-down on a reverse slope;
  - hull tilt feeding into armour obliquity.
- **(c) Why it matters:** hull-down fighting in the balkas (steppe ravines) and inaccuracy on the move.
- **(d) Effort:** M.

### 2.6 Engine start, temperature and wear
- **(a) What the code does**
  - Starting is a fixed 1.8 s crank whenever there is fuel and the engine module works (game.rs:13780-13800).
  - There is no coolant, oil or outside temperature, and no battery.
  - `tiger_damage` defines no engine or transmission faults (tiger_damage.rs:20-48).
- **(b) What is missing:** the inertia starter, preheating, coolant freezing, running engines through the night, overheating, final-drive and gearbox wear.
- **(c) Why it matters:** at −20 to −30 °C a cold start was long and noisy. Final drives and transmissions were the Tiger's chronic weakness.
- **(d) Effort:** M.

### 2.7 Running gear and tracks
- **(a) What the code does**
  - Each side has a running-gear value that scales top speed between 0.5 and 1.0 and pulls the steering by up to ±0.08 (tiger_damage.rs:405-411; game.rs:13917-13919).
  - A track is simply working or broken.
  - *Inferred:* tracks break only from hits. Mines do not exist.
- **(b) What is missing:** a choice of 725 mm combat or 520 mm transport tracks; throwing a track on a hard turn or side slope; interleaved road wheels freezing solid overnight; wheel changes.
- **(d) Effort:** S–M.

### 2.8 Fording, bridges and ice
- **(a) What the code does:** there is no water. The Act II canal is a no-go mask crossed by one 48 m bridge with no weight limit (game.rs:262-335). River ice does not exist.
- **(c) Why it matters:** 57 t exceeded most bridges of the period, and frozen rivers were decisive terrain.
- **(d) Effort:** M.

### 2.9 Turret traverse and elevation
- **(a) What the code does**
  - Powered traverse is a fixed 6°/s (tank.rs:18). It requires the engine running and both the engine and traverse modules working (game.rs:13895).
  - Hand traverse is 1.5°/s (game.rs:464, 13903).
  - Elevation is 4°/s (tank.rs:24), within −8° to +15° (anti_infantry_ai.rs:9-10).
  - AI traverse is 0.80 rad/s, which is 45.8°/s, although the comment says about 23°/s (tank.rs:19-22).
  - The Tiger's elevation limits are applied to every Soviet gun.
- **(b) What is missing:** the repo's own spec calls RPM-dependent traverse the "KEY GAMEPLAY HOOK" (TIGER_I_SPEC.md:29-34). Only the traverse *sound* reads RPM (audio.rs:2266-2296).
- **(d) Effort:** S once §2.2 exists.

### 2.10 Soviet vehicles
- **(a) What the code does:** every type shares the Tiger's speed, acceleration and turn rate. Types differ only in collision outline and mass (tank.rs:221-276). A KV-2 moves exactly like a T-34.
- **(c) Why it matters:** the T-34's wide tracks, low ground pressure and speed in snow were the defining asymmetry of that winter.
- **(d) Effort:** S as a parameter table, after §2.1.

### 2.11 Collisions (as they affect movement)
- **(a) What the code does**
  - **Tank against tank:** overlapping tanks are pushed apart by rectangles, split by mass, in 3 passes; speed drops to 34 % (game.rs:465-467, 7294-7314, 25350-25480). There is no momentum transfer and no ramming damage.
  - **Tank against buildings:** only the player's bow is tested, a single 1.4 m circle 3.2 m ahead (game.rs:1408-1410, 25492-25640).
  - **Enemy tanks:** they "treat walls as ghosts" (game.rs:25491).
  - *Inferred:* the collision outlines come from model bounds and are oversized: Tiger 4.12 × 7.13 m against a real hull of about 3.7 × 6.3 m.
- **(d) Effort:** M.

### 2.12 Firing on the move
- **(a) What the code does:** the player cannot fire above 0.6 m/s (tank.rs:877-879). That is a rule, not physics. Enemies can fire on the move, with spread that grows with speed.
- **(b) What is missing:** spread derived from how the hull is moving, which needs §2.5.
- **(d) Effort:** S after §2.5.

---

## 3. Ballistics and armour

### 3.1 Where a shell can hit
- **(a) What the code does**
  - Each vehicle is two boxes (game.rs:28545-28595):
    - Tiger hull: ±2.06 m wide, 0–2.595 m high, ±3.563 m long.
    - Tiger turret: ±1.51 m wide, 1.08–3.21 m above the hull floor.
  - The box face that is struck selects the front, side or rear plate (game.rs:28647).
  - **A contact through the top or bottom face is thrown away** (game.rs:28684-28686; armor_calibration_tests.rs:309, "roof/belly are not represented").
  - For component damage only, tiger_damage re-reads hull-box hits above 2.0 m as turret hits (tiger_damage.rs:162-202). This happens after the armour plate has already been chosen (game.rs:22165-22189).
- **(b) What is wrong or missing**
  - *Inferred:* steep plunging fire passes straight through the tank. A shallow shell entering the top face is picked up one step later, about 2.7–3.2 m inside, and scored as a front, side or rear plate hit (game.rs:28632-28643).
  - *Inferred from the box sizes:* the hull box's front face rises to 2.595 m, above the mantlet at about 2.37 m. So head-on fire at the mantlet resolves against the 100 mm driver's plate, and the 120 mm mantlet only counts above 2.6 m.
  - There is only one side plate, 80 mm (ballistics.rs:221); the 60 mm lower side does not exist.
  - Road wheels are not spaced armour; hits there only roll component damage (tiger_damage.rs:188-194).
  - The gun barrel cannot be hit.
  - There are no shot traps.
  - Enemy tanks have no tracks and no components.
- **(c) Why it matters:** mines under the 25 mm belly, grenades and bottles on the deck, plunging HE, hull-down duels, and Soviet crews taught to aim at the running gear.
- **(d) Effort:**
  - S: add roof and belly plates, and cut the hull box at the roofline.
  - M: a 60 mm lower side band, with the road wheels as spaced armour.
  - L: armour from the real mesh.

### 3.2 Tiger armour table
- **(a) What the code does**
  - The Tiger I table (ballistics.rs:218-225):

    | Face | Thickness | Slope |
    |---|---|---|
    | Hull front | 100 mm | 9° |
    | Hull side | 80 mm | 0° |
    | Hull rear | 80 mm | 9° |
    | Mantlet | 120 mm | 0° |
    | Turret side | 80 mm | 0° |
    | Turret rear | 80 mm | 0° |

    All of it is rolled homogeneous armour (RHA). The comment at ballistics.rs:219 still says "Face-hardened early plates", which contradicts the code.
  - Sideways angling of the hull is modelled, with the angle capped at 75° (ballistics.rs:353-360).
  - The shell's descent angle and the hull's pitch and roll are ignored, described as a "level-shot approximation" (armor_calibration_tests.rs:243-247).
  - The code's own audit asserts `historical_fidelity_validated: false` (armor_calibration_tests.rs:299, 328).
  - At most 24 armour scars are kept, oldest evicted first, so old holes silently heal (armor_damage.rs:12, 171-173).
- **(b) What is missing:**
  - the steep 60 mm upper nose plate and the 100 mm lower nose;
  - the 60 mm lower sides;
  - the 25 mm roofs and belly;
  - the mantlet's varying thickness;
  - plate cracking.
- **(d) Effort:** S–M.

### 3.3 Shell types
- **(a) What the code does**
  - Defined rounds (ballistics.rs:638-825):

    | Round | Mass | Muzzle velocity | Filler | Penetration anchor |
    |---|---|---|---|---|
    | PzGr 39 | 10.2 kg | 773 m/s | 59 g | 120 mm at 30°, 100 m |
    | PzGr 40 | 7.3 kg | 930 m/s | – | – |
    | 88 mm HE | 9.0 kg | 820 m/s | 900 g | – |
    | 45 mm BR-240 | – | 760 m/s | – | – |
    | 45 mm BR-240P | – | 985 m/s | – | – |
    | 76 mm BR-350B | 6.5 kg | 655 m/s | 65 g | – |
    | 76 mm OF-350 | – | – | 621 g | – |

    Further late-war rounds are also defined.
  - **What is actually fired:**
    - The player fires only PzGr 39 or HE, chosen by one flag (game.rs:22526-22531). PzGr 40 is never fired: `ShellNature::PzGr40` lives only in reload.rs, which is unused foundation code (reload.rs:9), even though the inventory holds 4 APCR (campaign.rs:157).
    - Each enemy type fires a single AP round (game.rs:2353-2360). The T-34 uses OF-350 only against soldiers (mixed_combat_runtime.rs:600-668).
  - **Data bugs:**
    - BR-240P is a tungsten-core (APCR) round but is typed as ordinary AP (ballistics.rs:715-725).
    - The plate-thickness-to-calibre ratio for tungsten-core rounds uses the full calibre, not the core (ballistics.rs:346).
- **(b) What is missing:**
  - German: Gr 39 HL HEAT (check its issue date) and smoke.
  - Soviet: BR-350A (check its dates against BR-350B), shrapnel, HE fired at the Tiger to strip tracks and optics, 14.5 mm anti-tank-rifle rounds.
- **(c) Why it matters:** the repo's own sources cite Zabel's February 1943 report on a Tiger of Heavy Tank Battalion 503 (s.Pz.Abt. 503). It took 227 anti-tank-rifle hits and 25 gun hits, none of which went through, yet still lost optics, machine guns and radio.
- **(d) Effort:** S for the data fixes; M per new round, mostly for sourcing.

### 3.4 Flight and gunnery
- **(a) What the code does**
  - **Flight:**
    - Velocity decays exponentially, plus gravity (ballistics.rs:91-107).
    - **Every shell is deleted at 1,500 m**, with no impact and no sound (game.rs:2605, 21847-21858).
    - Crosswind drift comes from a steady wind per act (game.rs:1618-1636). Act I has 1.7–5.4 m/s (world.rs:59). The head/tail component is dropped.
  - **Player gun:** the shell leaves exactly along the barrel, with no dispersion and no gun jump (game.rs:22527). Recoil is visual only (tank.rs:922-926).
  - **Enemy tank fire:**
    - Spread is ±4–18 mrad, but applied along the world X and Y axes: a bug, see §15 (game.rs:23795-23800).
    - Enemies know the exact range, aim at the Tiger's centre and do not lead (game.rs:23450-23456).
    - Anti-tank guns lead with perfect range (at_guns.rs:414-416).
  - **Sight:** it has **no range setting**; the reticle has only the range-estimation triangles (shaders/crush.wgsl:139-165), and ballistics.rs:1222 says the player "must hold over".
  - PzGr 39 falls about 2.2 m at 500 m, about 9.4 m at 1 km and about 22 m at 1.5 km.
- **(b) What is missing:** air density, propellant temperature, drag that varies with Mach number, barrel wear, and error in enemy range estimation.
- **(c) Why it matters:** the 88's advantage was hitting at 1–2 km using the range drum on the TZF 9b. *Inferred:* at −30 °C air is about 18 % denser than standard, and cold charges lose muzzle velocity; range tables corrected for both.
- **(d) Effort:**
  - S: remove the cap, fix the spread, add player dispersion.
  - S: density and propellant-temperature terms.
  - S–M: a range drum per ammunition type.
  - M: a drag curve and enemy range error.

### 3.5 What happens when a shell hits
- **(a) What the code does**
  - Penetration falls with striking speed from one calibration point per shell (ballistics.rs:63, 127-137).
  - Each shell type turns into the plate by a fixed amount: 5°, 3° or 1.5° (ballistics.rs:313-321).
  - A shell much wider than the plate reduces the benefit of slope (ballistics.rs:336-349).
  - Results within ±8 % of the limit are decided by a seeded random roll, which the code calls "game stochasticity" (armor_calibration_tests.rs:311).
  - **There is no ricochet.** A failed penetration deletes the shell (armor_calibration_tests.rs:307).
  - **Shells shatter only on face-hardened plate** (ballistics.rs:491-495), so never on the all-RHA Tiger.
  - HE holes a plate only if it is thinner than 18·√(filler kg) mm (ballistics.rs:462-468). That is 17 mm for the 88, so HE never holes any plate in the table.
  - There are no fuzes: the APHE filler goes off at the entry point with no delay and no duds (game.rs:24266-24270).
- **(b) What is missing:** ricochet with the shell continuing (including off frozen ground), shatter that depends on hardness, fuze delay and duds, and plate cracking.
- **(c) Why it matters:** Soviet 76 mm APHE breaking up on thick German plate at close range is reported in period trials (source before use).
- **(d) Effort:** M each.

### 3.6 Fragments behind the armour (spall)
- **(a) What the code does**
  - One fragment set per penetration (game.rs:3221-3355): 220–768 grains, 1.4–9 mm across, **speeds clamped to 7–220 m/s** (game.rs:3267-3268).
  - Fragments fly through nothing until they reach a box/cylinder stand-in for the cabin (game.rs:3855-3867, 7077-7082).
  - Components are spheres, knocked out by 0.18–5 J (game.rs:5968-5979).
  - Any fragment of ≥2.8 J crossing a live round detonates the whole load (game.rs:5973, 6123-6156).
  - **Any fragment of ≥0.24 J touching any part of the crewman kills him** (`CREW_LETHAL_ENERGY_J`, game.rs:5997; singleplayer_crew.rs:116).
  - The penetrator does not continue into the tank as a body of its own.
- **(b) What is wrong:** *inferred:* real behind-armour debris travels at hundreds of m/s. The 0.24 J floor (about a 1 g chip at 22 m/s) offsets the low speeds, so the result is a pair of non-physical numbers. Classic personnel-lethality criteria are tens of joules; about 80 J is the traditional figure.
- **(c) Why it matters:** whether the one crewman survives a hole is the whole single-player loop.
- **(d) Effort:** M–L, together with §6.2.

### 3.7 HE: fragmentation versus blast
- **(a) What the code does**
  - Against infantry, one rule (infantry_combat.rs:302-312; mixed_combat_runtime.rs:383-460): damage falls linearly to zero at 7 m·∛(filler/0.9 kg), from a 200 HP peak. A single line-of-sight ray to the torso decides cover, and posture is ignored.
  - Buildings take damage within a fixed 7 m, whatever the calibre (game.rs:1402, 22233).
  - Near misses cannot hurt vehicles or a head-out commander (mixed_combat_runtime.rs:274-283).
- **(b) What is missing:** separate models for blast (overpressure and impulse) and fragments (count, mass, velocity); absorption by ground and snow; bursts in treetops.
- **(c) Why it matters:** deep snow smothered HE while frozen ground made it more lethal, and artillery was the main source of attrition.
- **(d) Effort:** M.

**Ledger note.** PHYSICAL_TRUTH_AUDIT.md:71 still says the solver "clamps/overwrites entry geometry and samples damage separately". The code now says the entry point is "never clamped or overwritten" (game.rs:24184-24187), and one fragment solution owns damage, visuals, audio and replay (game.rs:3286, 5981-5983). What remains true:
- the entry point sits on the box face, not the real plate;
- fragments ignore geometry before reaching the cabin stand-in;
- the stand-in clamps fragments to its floor (game.rs:3200).

---

## 4. Damage to the player's Tiger

### 4.1 Components
- **(a) What the code does**
  - **Exterior strikes, penetrating or not:**
    - The contact is zoned on the two armour boxes (tiger_damage.rs:162-201).
    - The effect is rolled by a seeded hash per calibre class and zone (tiger_damage.rs:217-224, 261-355), applied at game.rs:22399 → 24101-24135.
    - Faults each carry a field fix from the Tigerfibel, the Tiger crew manual (tiger_damage.rs:20-81).
  - **Internal modules:** 12 binary modules plus a fault (game.rs:5125-5230). They are damaged only by penetration fragments crossing fixed spheres of 0.40–0.85 m radius (game.rs:5198-5206).
- **(b) What is missing or simplified**
  - `Strike.energy_kj` and `.penetrated` are **never read** by the effect table (tiger_damage.rs:207-208). A 76 mm hit at 50 m and one at 2 km have identical odds.
  - The hull-wall and turret-wall zones (sides, rear, roof, deck) only roll radio or sight "shock" (tiger_damage.rs:340-353). Nothing outside can damage the engine, radiators, fans, grilles or exhausts.
  - Non-penetrating hits make no spall (game.rs:3228-3239), although ballistics.rs:375 describes HE as causing "interior spall shock".
  - The anti-tank-rifle damage class (`Class::Rifle`) is unreachable: no AT rifles exist, and small arms are inert against armour (small_arms_runtime.rs:119-120).
  - Modules that do not exist:
    - final drives;
    - the L 600 C steering unit;
    - radiators and fans;
    - batteries, generator and electrics (the KwK 36 fired electrically);
    - the driver's and loader's periscopes;
    - hatches;
    - the separate fuel tanks.
  - Legacy repairs are too quick: a wrecked transmission is fixed with a hammer in 7 s, a track in 8 s (game.rs:7198-7207; equipment.rs:261-268).
  - A radio fault only feeds a debug readout (mechanic_readout.rs:23).
  - Running gear never immobilises: the speed factor stays at 0.5 or more (game.rs:13917-13918).
  - *Inferred:* each act is rebuilt fresh (game.rs:11975-11977), and only binary modules, scars and stores carry over (game.rs:12020-12072, 12038, 12189).
- **(c) Why it matters:** in 1943 a Tiger usually lost fighting power to hits that did not penetrate: optics, MGs, radio, turret ring and running gear. Mechanical failure took more Tigers out of service than penetrations did.
- **(d) Effort:**
  - S–M: effects that depend on energy and penetration, plus rear, roof and deck zones.
  - M: non-penetrating spall and the missing modules.
  - S: replace the 7 s transmission repair.

### 4.2 Fire and ammunition
- **(a) What the code does**
  - **Ignition:** fire starts only when fragments reach the Fuel or Engine spheres (game.rs:6157). Those spheres lie outside the fragment compartments: the hull box spans z 0.28–2.30 m and the turret cylinder has r = 1.04 m (game.rs:7077-7082).
  - *Inferred:* they can be reached only during the collision-free flight before a fragment enters the compartment (game.rs:3855-3867), so fragments pass straight through the firewall.
  - **Where fire burns:** it then anchors at fixed crew-compartment spots, whatever was hit (game.rs:5297-5302, 16750-16753).
  - **Growth:** a dimensionless authored model (surface_fire.rs:3, 1195-1299). Venting with the hatch open *slows* growth, 0.56 vs 0.76 per second (surface_fire.rs:1253). *Inferred:* that is backwards for a fire starved of oxygen.
  - **Cook-off:** exposure of 0.65 or more starts a fixed 3.0 s fuse (ammunition_fire.rs:4-5), which ends in a whole-tank kill (game.rs:24703-24765).
  - **Detonation:** the summed filler of every live round goes off at once, with propellant ignored (game.rs:20356-20366, 20465-20468).
  - **Extinguisher:** unlimited (game.rs:27372-27385). The inventory's `extinguishers: 2` is never read (campaign.rs:163).
  - **An exploding Tiger harms no one outside:** `spawn_blast` is visual only (game.rs:24956-24970).
- **(b) What is missing:**
  - an engine bay and firewall;
  - fuel in the damage model;
  - the automatic engine-bay extinguisher;
  - propellant burning separately from HE detonating;
  - fire, heat and smoke harming the crew.
- **(c) Why it matters:** the Tiger ran a petrol Maybach and had engine-bay fires; the Tiger I carried an automatic extinguisher in the engine bay. *Inferred/general:* brass-cased propellant usually burned fiercely rather than detonating as a whole load.
- **(d) Effort:**
  - S: fire harming the crew, and a finite extinguisher.
  - M: engine bay, firewall, fuel and the automatic extinguisher.
  - M: a propellant-versus-HE model.

### 4.3 HP numbers that still decide outcomes
- The player Tiger's `hp` is a derived readout, but it round-trips: at act import, `alive = hull_condition > 0` (game.rs:15703-15714, 12028-12029).
- Act III ally Tigers lose 38 HP per tick of a hostile-seconds clock (game.rs:13532-13553). The ledger already lists this as a P0.
- On foot the crewman has 100 HP (foot.rs:25) and loses 34 per rifle hit (game.rs:458). A medical pickup heals +34 instantly (game.rs:18204-18208).

---

## 5. Enemy vehicle damage (the ledger's P0 is confirmed)
- **(a) What the code does**
  - Any penetration of a live enemy calls `destroy_enemy` (game.rs:22335-22344). tank.rs:736-740 says it plainly: "enemies just die on a penetration."
  - Enemy tanks have no modules and no crew (tank.rs:719-741).
  - Non-penetrating hits, including all HE, only produce a clang (game.rs:22358-22362, 22202-22203).
  - The wreck outcome is random: 35 % turret pop, otherwise a 55 % chance of cooking off after 3–8 s (game.rs:628-633, 24826-24865).
    - It is seeded from a **global kill counter plus the enemy's slot index** (game.rs:24819-24823), not from the round, the hit point, the ammunition or the fuel.
  - Wrecks burn forever.
  - Each wreck adds a 6.8 × 6.8 × 2.6 m sight blocker that ignores its heading (game.rs:24805-24814). Yet wrecks do not stop shells (game.rs:21708, 21721-21737).
  - Ally Tigers also die on any penetration (game.rs:22364-22367).
  - A 45 mm gun is wrecked by any cannon contact, or by HE within 1.5 + 3√(filler kg) m (at_guns.rs:459-477).
  - Kill explosions hurt no one nearby (game.rs:24868-24891).
- **(b) What is missing:** modules, crews, bail-outs, immobilisation, track, gun and optic kills, stunned crews, knockouts that don't burn, burn-out, and Soviet recovery of wrecks.
- **(c) Why it matters:** a Tiger gunner could not know a T-34 was dead until it burned or its crew bailed out. Crippled tanks kept firing, and bailed crews fought on as infantry. That is exactly the wordless evidence the first principle asks for.
- **(d) Effort:** M–L for a shared vehicle damage state (the player's model applied to the T-34's layout). M as an interim: derive the outcome from the hit and its fragments, and add bail-outs.

---

## 6. Crew and human factors

### 6.1 What "one crewman" means in the code (commit dfaf4b4)
- **(a) What the code does**
  - The single-player roster has one man; the other four seats are "empty, not dead" (singleplayer_crew.rs:1-3, 39-47, 281-287). `crew_ok` ignores the role (game.rs:15674-15677).
  - **Station changes are instant in live play.** `jump_station` sets `switch_t = 0` (game.rs:22856-22875). The timed `request_station` costs, 0.55 s within the turret and 0.95 s between hull and turret, are used only by headless and AI paths (game.rs:6753-6754, 7161-7167, 15719-15733).
  - **He can drive from every view except the loader's walk:** "views are instruments, not seats" (game.rs:13812-13826).
    - From the sight he lays and fires both the main gun and the coax.
    - From the head-out cupola he fires the hull MG (game.rs:14009-14029).
    - He can start the engine from any post (game.rs:13771-13773).
  - This contradicts TIGER_I_SPEC.md:46-47 ("no gun override, no driving controls" at the commander's post). The ledger does not name it.
- **(c) Why it matters:** one man in a five-man machine should pay in time, and in blindness, to move between stations.
- **(d) Effort:** M. S just to name the abstraction in the ledger.

### 6.2 Injury and bleeding
- **(a) What the code does**
  - Survival is binary: a fragment of 0.24 J or more anywhere on the body kills (§3.6).
  - crew_damage grades tissue outcomes, but they only drive blood and ragdolls (crew_damage.rs:131-143, 185-218).
  - The only per-person state is `Crewman { ok: bool }` (game.rs:2941-2944).
  - Blast is an inverse-square fluence × body area × a coupling constant of 0.0025 (crew_damage.rs:33, 248-257).
  - Nothing exists for bleeding, blood volume, shock, consciousness or treatment.
- **(b)** STATUS.md:197-200 already locks this design, but it is unbuilt.
- **(c) Why it matters:** most casualties in a penetrated tank were wounded, not killed. A wounded lone crewman is the drama.
- **(d) Effort:** L.

### 6.3 Head-out exposure
- **(a) What the code does:** exterior projectiles test the player's body only on foot (game.rs:21712-21719). HE and small arms likewise target the player only on foot (mixed_combat_runtime.rs:274-283, 436-447). Mounted, infantry are passive (game.rs:18886-18909).
- **(c) Why it matters:** commanding head-out was doctrine, and commanders were lost to snipers and fragments.
- **(d) Effort:** M (the anatomical volumes already exist).

### 6.4 Cold
- **(a) What the code does:** no air, interior or body temperature exists. The only "temperatures" in the code are light colour (environment_authoring.rs:76), and ammunition_fire.rs:2 states its exposure "is not a temperature". There is no visible breath.
- **(b) What is missing:** frostbite, hypothermia, cold hands slowing loading and repairs, engine heat warming the crew, open hatches draining heat.
- **(c) Why it matters:** December–January 1942–43 ran roughly −20 to −35 °C.
- **(d) Effort:** M–L.

### 6.5 Fatigue and sleep
None exist. **(c)** Crews went days without sleep, and loading and vigilance decay with it. **(d)** M.

### 6.6 Hunger and thirst
- **(a)** Rations, cigarettes and medical supplies are tallies (foot.rs:276-317; campaign.rs:146-147, 164-165). They are never consumed, except by the instant heal.
- **(c)** Famine is the Stalingrad context.
- **(d)** S–M.

### 6.7 Fumes and carbon monoxide
- **(a)** `interior_haze` is a visual scalar. It rises +0.11 per shot (game.rs:22744) and +0.035 when the breech vents (game.rs:16214-16223). It drains at 0.030/s, plus 0.40/s with the hatch open (game.rs:16720-16728). Only the renderer reads it.
- The turret-roof extractor fan exists as geometry (interior_rig.rs:20-21), but has no switch, electrics or failure.
- **(c)** Firing buttoned up fouled the turret; crews got headaches and opened hatches under fire.
- **(d)** S–M.

### 6.8 On foot
- **(a)** Speeds are fixed at 2.6 / 5.5 / 1.3 m/s (foot.rs:18-23). Sprint is unlimited, carried mass has no effect (game.rs:17952-17962), snow depth is ignored, and gravity is 14 m/s² (foot.rs:28).
- **(c)** Knee-deep snow while carrying a ~20 kg round or a 20 L jerry can.
- **(d)** S–M.

### 6.9 Morale and stress
- None exist. "Concussion" is a post-process effect of about 2.25 s (crush.wgsl:78-89; game.rs:5625-5636). **(d)** M.

### 6.10 Hearing
- See §1.5. Hearing belongs to the listener, not the crewman (game.rs:8039-8040), and is not saved.

---

## 7. Weather and environment

### 7.1 Weather fronts
- **(a) What the code does**
  - A seeded cycle of 80–120 s legs alternates lull (storm 0.12–0.24) and blizzard (0.90–1.0). The file header calls these "art-directed weather cycles, not a meteorological forecast" (winter_weather.rs:1-2, 17-37).
  - Fronts run only in Act I winter (game.rs:14068-14079).
- **(c) Why it matters:** steppe blizzards (burans) lasted hours to days. Weather windows decided operations and grounded the airlift.
- **(d) Effort:** M, as an hourly scenario timeline per act.

### 7.2 Temperature
- **(a) What the code does:** there is none (see §6.4).
  - Constants assume about 20 °C: 343 m/s.
  - Engine start is fixed.
  - `Season` changes only atmosphere and precipitation (game.rs:14064-14079).
- **(b) What one scalar would unlock:**
  - speed and absorption of sound;
  - air density and propellant muzzle velocity;
  - cold starts, batteries and grease;
  - crew cold;
  - frost on optics;
  - ice strength.
- **(d) Effort:** S for the scalar plus the sound and drag hookups; M for the other consequences.

### 7.3 Wind
- **(a) What the code does**
  - Each act has a fixed wind vector (world.rs:56-73). Act I is westerly, 1.7–5.4 m/s, with ±20 % spatial variation.
  - Shelter applies only near the legacy `CITY_CENTER` (game.rs:1453, 1595, 1618-1636). Fronts scale the wind by 0.40–1.38 (winter_weather.rs:35).
  - Wind feeds shell crosswind drift, smoke, snow, grass and the wind-noise bed.
  - Direction never veers. Gusts exist only inside the audio bed (weather_audio.rs:105-113).
  - Wind never enters sound propagation or AI hearing.
  - `weapon_vfx.rs:234` uses the unscaled act wind while volumetric smoke uses the scaled one.
- **(d) Effort:** S for gusts and veer; M for coupling into propagation.

### 7.4 Day and night
- **(a) What the code does**
  - There are four fixed phases (renderer.rs:770-818), and the campaign offers only Day or Night from the menu (game.rs:1610-1616). No clock runs within an act.
  - **Night = solar irradiance × (1 − 0.80)** (renderer.rs:6591), with the light source at about 35° elevation (renderer.rs:814, *inferred* from the vector).
  - The raster path mixes 20 % of the day sun with a fixed "moon" (shaders/mesh.wgsl:354-360).
  - The day sun sits at about 31.5° (renderer.rs:812).
- **(b) What is missing:** an ephemeris (date, latitude, sunrise and sunset), moon phase, starlight, and flares, star shells or burning villages as the night's light.
- **(c) Why it matters:** *inferred astronomy:* at 48.7°N the noon sun is about 18° at the solstice, and daylight lasts about 8 h. A 31–35° sun belongs to late winter or spring. Moonlight is roughly 10⁻⁵ of daylight. "Night" at 20 % of the Sun is a brightness floor, which CLAUDE.md principle 2 rule 1 forbids.
- **(d) Effort:** S for an ephemeris and moon; M for time passing within an act.

### 7.5 Visibility compared with AI sight
- **(a) What the code does**
  - The renderer's horizontal transmission is exp(−σd), with σ = 0.0068 m⁻¹ in Act I (renderer.rs:654-668, 733-736).
  - The AI uses separate constants:
    - tanks: 450 m for a still Tiger and 800 m for a moving one, ×0.35 at night (stealth.rs:19-21);
    - winter clamp: 360 − 120·storm m (winter_encounter_ai.rs:142-146; applied at game.rs:23551-23552).
  - Line of sight is one ray between hull centres (game.rs:23598-23613; tank.rs:973-975).
  - Smoke does not block AI sight.
- **(b)** *Inferred* (Koschmieder's 3.912/σ): the rendered frame shows about 0.6–1.5 km, while T-34 crews are capped at about 250–350 m.
- **(d) Effort:** M.

### 7.6 Snow on the ground
- **(a) What the code does:** there is no depth field. physical_environment.rs:5-6 says "Persistent snow transport … is not yet wired." The surface zones are editor-only (§2.4).
- **(c) Why it matters:** snow was scoured off rises and drifted deep in the balkas. The T-34 had the advantage there, and a man in knee-deep snow manages about 1 m/s.
- **(d) Effort:** M for a static depth map; L for accumulation and drift over time.

### 7.7 Frozen ground, ice and water
- **(a) What the code does:** Act I has no rivers or ice (§2.8). A shell that hits terrain simply ends, with no ricochet and no dependence on ground state (game.rs:22296-22310).
- **(c) Why it matters:** Operation Wintergewitter crossed the frozen Aksai and Myshkova. s.Pz.Abt. 503 fought on the Manych and the lower Don in January 1943.
- **(d) Effort:** M for ice; S for ground-state effects on HE and ricochet.

### 7.8 Thaw and mud
- **(a)** None at runtime; the mud zones are editor-only.
- **(c)** Low priority for December–January.
- **(d)** S–M.

---

## 8. Infantry and AI

### 8.1 How enemies detect the Tiger
- **(a) What the code does**
  - **Tank crews**
    - A detection meter fills over time (stealth.rs:19-45, 247-294).
    - Arc weights are 1.0 front, 0.3 side, 0.1 rear.
    - Modifiers (game.rs:23528-23549): open hatch ×1.18; engine off ×0.42; just after an engine start ×1.7; player on foot ×0.34.
    - Crew skill scales the rate (game.rs:23579-23580).
  - **Anti-tank guns:** they see the Tiger instantly when it is in reach and in line of sight, with no night factor (at_guns.rs:367, 375-380). Their "alerted" state is read from global game state (at_guns.rs:364-365), a hidden omniscient coupling.
  - **Infantry**
    - They see a fixed 45 m over ±70°, with 5 s of memory (infantry_combat.rs:12, 391-401, 433).
    - They are **deaf**: `observe` takes no noise input (infantry_combat.rs:405-503).
    - They **never perceive the Tiger itself**: the observation hard-codes `tiger: RelativeCue::default()` and `target_armored: false` (infantry_combat.rs:476-485).
  - **Spawning:** campaign encounters trigger at 430 m or more (campaign.rs:852), and every tank spawns already facing the player's hidden position (game.rs:12899).
- **(b) What is missing:** whitewash, silhouette against snow or sky, exhaust vapour in cold air, snow spray, the 88's muzzle-blast signature, and tracks in the snow (which are render-only).
- **(d) Effort:** M. S for the anti-tank gun and spawn fixes.

### 8.2 Hearing and communication
- **(a) What the code does**
  - **Hearing exists only in the live-playtest winter encounter.** Campaign tanks never hear the Tiger (live_playtest.rs:97-98).
  - Ranges are fixed thresholds:

    | Source | Heard out to |
    |---|---|
    | 88 mm | 900 m (game.rs:21637) |
    | Rifle or MG | 520 m (small_arms_runtime.rs:63-64) |
    | P08 | 240 m (small_arms_runtime.rs:63-64) |
    | 45 mm gun | **1,500 m** (at_guns.rs:438) |

  - Obstruction multiplies range by 0.62 (winter_encounter_ai.rs:187).
  - A sound yields only an area on a 32 m grid, remembered for 150 s (winter_encounter_ai.rs:31, 189-192).
  - **Communication:** platoon leaders' radios relay after 5 s (winter_encounter_ai.rs:282-302). An infantry alarm shout carries 40 m (game.rs:457, 18862-18879).
- **(b) What is wrong or missing**
  - Hearing is not tied to the acoustics model.
  - No continuous sources are heard: engine, tracks, traverse.
  - The ranking is inverted: the 45 mm carries farther than the 88.
  - No reports from infantry to tanks, or from observers to artillery.
  - The campaign's `battle_pressure` is a map-wide scalar with no propagation (game.rs:22548, 23781, 24802, 13160, 13064).
- **(d) Effort:** M, reusing §1.8's propagation.

### 8.3 Suppression, morale, surrender and retreat
- **(a) What the code does**
  - Suppression is set to 1.0 only when a soldier is hit, and decays at 0.8/s (infantry_combat.rs:348, 516).
  - Live HE never suppresses (mixed_combat_runtime.rs:383-462), although training's HE does (infantry_combat.rs:367).
  - Nothing implements morale, surrender, flight or rout.
  - Soviet soldiers can only stand (infantry_combat.rs:121-125).
  - Tanks never bail out, retreat or break off.
- **(c) Why it matters:** wordless storytelling runs on how men react. The mass surrenders of 31 January–2 February 1943, and crews bailing out and fleeing, are attested.
- **(d) Effort:** M for suppression, pinning and bail-out; L for morale and wounded behaviour.

### 8.4 Infantry anti-tank tactics
- **(a) What the code does**
  - Infantry weapons are only rifle, P08 and MG (infantry_combat.rs:75-79). Small arms are inert against armour (small_arms_runtime.rs:119-120).
  - Infantry never target the mounted Tiger (game.rs:18886-18908).
  - There is no crew projectile contact (game.rs:2784-2797).
  - *Inferred:* infantry cannot be run over, and the Tiger cannot crush a 45 mm gun (game.rs:21751, 22235).
- **(b) Missing:**
  - PTRD-41 and PTRS-41 anti-tank rifles;
  - RPG-40 grenades;
  - bottles (KS or Molotov) on the engine deck;
  - bundle charges;
  - tank-hunter teams that climb aboard;
  - mines (TM-41, TMD-B);
  - PPSh-41 and DP-28;
  - anti-tank dogs (only with firm sources).
- **(c) Why it matters:** anti-tank rifles could not penetrate a Tiger, but they destroyed vision blocks and MGs and wounded exposed commanders (Zabel). Mines immobilised Tigers, and bottles on the intakes caused engine fires.
- **(d) Effort:** M for an AT-rifle team (a 14.5 mm round through the existing flight path into `tiger_damage`'s rifle class). M for mines. L for grenade and bottle teams.

### 8.5 T-34 tactics and fire control
- **(a) What the code does**
  - **Winter encounter:** platoons of 3 and 2 at 40 m spacing; the leader holds 165–235 m off; a 650 m leash (winter_encounter_ai.rs:20-25, 315-320, 352, 361-396).
  - **Campaign:** tanks orbit and flank left or right by index parity (game.rs:23673-23714).
  - `ai_doctrine` is dead code (ai_doctrine.rs:7).
  - The evolved MLP policy is `None` in normal play (game.rs:11798). Only the headless modes install it (main.rs:2495, 8162, 8401, 8750).
  - Reload is 5.5 s × (1.16 − 0.42·skill) (game.rs:459, 23473). *Inferred:* enemy AP ammunition is unlimited.
  - Aiming uses the exact range, the Tiger's centre and no lead (§3.4).
- **(b) What is missing:**
  - cover and hull-down;
  - smoke;
  - overwatch;
  - aimed shots at tracks and vision ports;
  - flank rushes to 100 m;
  - tank riders;
  - range-estimation error.
- **(d) Effort:** S for the dispersion bug and range error; M–L for doctrine.

---

## 9. Soviet armour, anti-tank guns, artillery and aircraft

### 9.1 Armour roster
- **(a) What the code does**
  - `EnemyKind` has six types: T34, Su100, Kv2, Isu152, Is2, Is3 (tank.rs:82-89).
  - Act 1 in the campaign fields only T-34s, plus one KV-2 "boss" (campaign.rs:1020-1028).
  - The winter encounter fields five T-34s (live_playtest.rs:36-50).
- **(b) Missing for winter 1942–43:**
  - KV-1 and KV-1S;
  - T-70 and T-60;
  - Lend-Lease Matilda II and Valentine, and M3s on some southern fronts (verify unit by unit);
  - BA-64 and BA-10 armoured cars;
  - trucks and horse transport.
- **(d) Effort:** M per vehicle.

### 9.2 Anti-tank guns
- **(a) What the code does**
  - The only gun is the 45 mm 53-K. Its two guns appear only in the live-playtest encounter, never in the campaign (live_playtest.rs:66-74).
  - Doctrine is in at_guns.rs:14-43, 113-168. Tungsten rounds are used only for flank shots at 350 m or less (at_guns.rs:190-210).
  - They aim with perfect range and lead (at_guns.rs:414-416).
  - The M-42 and ZiS-2 specs exist but are used only by captures and tests (astra_capture.rs:81-82; game.rs:19619).
- **(b) What is missing:**
  - the ZiS-3 76 mm, the main gun threat to a Tiger's side;
  - the 76 mm regimental gun;
  - the 85 mm 52-K anti-aircraft gun used against tanks;
  - 122 mm howitzers firing directly;
  - dug-in pits and relocation;
  - range error.
- **(d) Effort:** S–M: parameterise `AtGun` by gun type and put the guns into the campaign.

### 9.3 Artillery, mortars, aircraft and friendly forces
- **(a) What the code does**
  - None of these exist; the only artillery is aftermath scenery (environment_authoring.rs:2066-2213).
  - German riflemen spawn behind the Tiger (game.rs:11452-11467).
  - Ally Tigers exist only in the 1945 finale (game.rs:7225-7233).
- **(b) What is missing:**
  - 82 and 120 mm mortars;
  - 76, 122 and 152 mm fire with observers and adjustment;
  - BM-13 rocket launchers;
  - Il-2 aircraft with bombs, RS-82 rockets and cannon (PTAB bomblets only from July 1943);
  - the Luftwaffe airlift overhead as wordless evidence;
  - German fire support.
- **(c) Why it matters:** indirect fire dominated the soundscape and forced crews to button up. It stripped tracks, optics and antennas and wounded exposed men. It is the largest missing *loudness* source.
- **(d) Effort:** M–L for mortars and artillery; L for aircraft.

### 9.4 Anachronisms (CLAUDE.md principle 1.2)
- Panzerfaust 60 in Act 1: six tubes in the inventory (campaign.rs:160), a rack of three in the Tiger (game.rs:456), and the model itself (panzerfaust.rs:1). The first Faustpatrone arrived around August 1943, and the 60 in 1944.
- SU-100 in the default swarm (game.rs:1492-1499), used by sessions without anchors (main.rs:11969-11974).
- A KV-2 as the Act 1 boss: production ended in 1941 and nearly all were lost that year.
- The T-34 fires BR-350B (game.rs:2355); check it against BR-350A for 1942–43.
- IS-3 in Act 3 (campaign.rs:1025-1026) saw no combat in WWII.
- No Tigers fought inside Stalingrad; the nearest real case is s.Pz.Abt. 503 on the Manych–Rostov axis in January–February 1943. The project's own brief says so.
- **(d) Effort:** S each.

---

## 10. Logistics

### 10.1 Ammunition
- **(a) What the code does**
  - There are 65 physical stowage slots (interior_rig.rs:115-123), against 92 rounds historically. The campaign starts with 34 AP, 12 HE and 4 APCR (campaign.rs:155-157), and the APCR can never be fired.
  - **Probable bug:** 49 of the 65 slots are `Frame::Turret`, placed 1.31–1.56 m from the turret axis (`SHELL_SLOTS` in interior_rig.rs). They are rendered and hit-tested with `turret_model()` (game.rs:16425, 20376-20380). *Inferred from the geometry:* those are sponson positions outside the ~0.9 m ring radius, so the racks and their detonation boxes swing around with the turret.
  - Resupply:
    - per act, 5 ammo, 4 fuel, 3 medical and 2 cigarette caches (game.rs:337-450);
    - an ammo box gives 6 AP rounds plus 12 Luger rounds (game.rs:8395-8398, 18205);
    - 12 rounds per authored Tiger wreck (wreck_salvage.rs:1-4).
  - Supplies are weightless. They are picked up automatically within 1.4 m (game.rs:18196-18203) and stowed instantly on remount (game.rs:17250-17258).
  - Dead Soviets drop 9 mm rounds (game.rs:18227-18254); Soviet troops carried 7.62 mm.
  - MG ammunition is never consumed (`mg_rounds: 1_800`, campaign.rs:158; game.rs:23220-23250).
- **(d) Effort:** S.

### 10.2 Fuel
- **(a) What the code does**
  - Burn is (0.018 + 0.010·|v|) L/s, "a gameplay calibration, not a measured Maybach fuel map" (standalone_fuel.rs:9-16). That is 65 L/h at idle, 11.5 L/km at 12 m/s and 13.6 L/km at 5 m/s.
  - Capacity is 540 L (game.rs:233); historically 534 L. The campaign starts with 420 L (campaign.rs:161).
  - A jerry can transfers instantly (game.rs:17731-17755).
  - A hit on the fuel module starts a fire but never drains fuel (game.rs:6157, 6218).
- **(b) What is wrong:** the repo's own spec gives about 110–120 km on roads and 60–80 km cross-country on ~534 L, i.e. roughly 4.5–8.9 L/km (TIGER_I_SPEC.md:18). The game burns about 1.5–3× that. Burn ignores gear, load, slope, snow and cold, and there are no leaks.
- **(c) Why it matters:** fuel was the scarcest resource, and captured Soviet diesel was useless in a petrol Maybach.
- **(d) Effort:** S to recalibrate now; S–M for power-based burn once §2.2 exists.

### 10.3 Spares, repair, recovery and maintenance
- **(a) What the code does**
  - `spare_parts: 3` is used only to patch armour holes (game.rs:18164-18179; armor_damage.rs:192-201).
  - There are 6 spare links on the bow plate and 4 spare vision blocks (tiger_damage.rs:390-393).
  - `FieldRepair.spare` is always `None` (tiger_damage.rs:68-80), so MG barrels and recoil oil are infinite.
  - The track ritual is one man, one link and four hammer blows (game.rs:18334-18445). *Inferred:* a Tiger combat track run weighs about 2.5–3 t, and several men re-laid it using the engine and a tow cable.
  - Absent:
    - repairs to road wheels, torsion bars and idlers;
    - towing and recovery;
    - wear and breakdowns;
    - coolant, oil, antifreeze and batteries;
    - workshops between acts;
    - demolition charges.
  - The only way to restore the tank is to swap into an abandoned spare Tiger (game.rs:17282-17292).
- **(c) Why it matters:**
  - Recovering a Tiger took two or three 18 t Famo half-tracks (Sd.Kfz. 9), and towing a Tiger with another Tiger was forbidden.
  - Crews were ordered to blow up Tigers they could not recover. By common account, the first Tigers the Red Army captured (January 1943, near Leningrad) had been abandoned.
- **(d) Effort:** L for reliability and recovery; S for spares that repairs actually consume.

---

## 11. Terrain and destruction
- **Heightfield**
  - (a) Act I is an authored 1024² grid at 4.29 m spacing (terrain.rs:449-470), derived from a 25.22 m/px DEM (terrain.rs:1-14). Balkas and craters are carved offline (tools/worldgen/gen_world.py:107-134, 554-573).
  - The hull stays upright (§2.5).
- **Craters**
  - (a) Heights never change during play. HE ground bursts are visual effects plus damage (game.rs:22215-22231). There are no scorch marks or thrown earth on the snow.
  - (c) Shell-pocked, soot-stained snow is exactly the material evidence the first principle asks for.
  - (d) M for runtime height changes, with collision, sight and navigation refreshed locally; S for scorch marks.
- **Track marks**
  - (a) A rut map is stamped every frame for every tank moving faster than 0.25 m/s (renderer.rs:3292-3307, 5549-5563; game.rs:14251-14268). It affects shading only, never decays, and the AI cannot read it.
  - (d) S for decay under snowfall; M for a track memory the AI can read.
- **Buildings**
  - (a) Destructible pieces:
    - at most 18 walls from the old city generator (game.rs:811, 8660-8683);
    - at most 44 other destructibles in total (game.rs:810);
    - one featured 8 × 8 m building with support topology (building_runtime.rs:1-7).
  - All of it is brick; the timber, stone and bunker materials exist only in tables and tests (game.rs:931-941).
  - About 11 whitelisted asset types get box proxies (worldgen.rs:431-460, 563-667). They become indestructible hard cover (game.rs:10678-10697, 21760-21762, 25586-25600).
  - (c) Stalingrad was masonry combat.
  - (d) L.
- **Placements with no physical presence**
  - (a) *Inferred* from the classifier and my count of Act I placements: about 3,100 have no collision, no sight blocking and no effect on shells, including 2,528 brick piles and debris and 183 broken pillars (worldgen.rs:667).
  - About 4,800 floor and roof slabs support only walking (walk_surfaces.rs:30-36).
  - (d) M: source-derived proxies of the kind manifest_wreck_collision.rs already builds.
- **Cover and line-of-sight stand-ins**
  - (a) Non-destructible old-city pieces are merged into 22 m-cell boxes for line of sight (game.rs:8630-8636, 8684-8706).
  - Soviet riflemen judge sight against six hard-coded 2D cylinders at old coordinates (game.rs:11351-11361; infantry.rs:832-857). *Inferred:* they see through every wall in the map, while their firing check does include those walls (game.rs:12543-12557).
  - A door leaf stops an 88 mm round (game.rs:21694-21697).
  - (d) M; S for the doors.
- **Rubble**
  - (a) A collapse shows a heap and lowers the sight box to 0.6 m (game.rs:916-921, 26029-26042). Debris is frozen with no collision (rubble.rs:12-14), and heaps stop no projectiles (game.rs:21765).
  - (d) M.
- **Crushing**
  - (a) Only the player's Tiger crushes, testing one 1.4 m circle placed 3.2 m ahead. Material tiers are at game.rs:951-976; trees at trees.rs:318-376.
  - *Inferred:* the hull's sides and rear clip through walls. The Tiger cannot crush a gun (at_guns.rs:237-240) or a soldier. Wrecks cannot be pushed (game.rs:25345-25349).
  - (c) Tigers routinely overran anti-tank guns.
  - (d) M.
- **Trees**
  - (a) Any main-gun hit fells any tree (trees.rs:282-293), and HE fells every tree within 7 m. Fallen trunks drop out of every query (trees.rs:1-5).
  - (d) S–M.
- **Wrecks**
  - (a) Killed tanks do not stop shells but do block sight (§5). Authored static wrecks use voxel proxies and do stop shells (manifest_wreck_collision.rs:1-3).
  - (d) S.

---

## 12. Optics and vision
- **Gunner's sight**
  - (a) Three fields of view: 25°, 12° and 6° (game.rs:8063-8069, 14584). The 6° stage is labelled "HONEST COMPENSATION" for the pixel grid.
  - The code says it has no TZF prescription (tiger_gunner_sight.rs:1-3), and that its "zoom steps are uncalibrated" (crush.wgsl:146).
  - The reticle is drawn in post-processing (crush.wgsl:139-165). There is no range drum (game.rs:21102-21104).
  - (b) Winter 1942–43 Tigers had the binocular TZF 9b at a fixed 2.5× (about 23–25°). The 12° stage belongs to the 1944 TZF 9c (TIGER_I_SPEC.md:41-43). Range scales per ammunition type are missing.
  - (d) M.
- **Cupola**
  - (a) Seven periscope blocks through a "declared ideal relay" (tiger_periscopes.rs:32-44, 306-367), with 64° of free look (game.rs:15864-15878).
  - (b) Winter 1942–43 Tigers had the drum cupola with five vision slits (TIGER_I_SPEC.md:46).
  - (d) L (asset work).
- **Driver**
  - (a) Visor shutters and a 48° view (tiger_driver_visor.rs:48-52). Leaning with the shutters open switches to an "accessibility relay": a 90° camera at z 2.74 m, **outside the armour** (tiger_driver_visor.rs:13-27; game.rs:15778-15829). It is not debug-gated and not in the ledger.
  - There is no KFF 2 binocular driver's periscope.
  - (d) S to gate the relay; M to add the KFF 2.
- **Radio operator**
  - (a) A 54° view with a fixed MG bore (game.rs:15750-15755; tiger_front_optics.rs:60-100).
  - (b) Missing: the KZF 2 MG sight (about 1.75×) and the ball mount's traverse.
  - (d) M.
- **Binoculars**
  - (a) A 10° field with a 0.45 s raise (game.rs:480-485). They can be used buttoned up at the commander's post (game.rs:14146-14151), which is dubious through a vision block.
  - (d) S.
- **Night and eye adaptation**
  - (a) A fixed exposure of 320, enforced by a test (renderer.rs:750-754, 2272-2273, 10655-10662). There are no flares; cupola_pistol.rs is the P08.
  - (b) Missing: dark adaptation over minutes, night-vision loss of colour and sharpness, flash blindness from the muzzle flash.
  - (d) M; L for a physical night sky (§7.4).
- **Weather on glass**
  - (a) Only combat damage degrades optics (equipment.rs:5-10; tiger_damage.rs:53-56). There is no frost, fogging, breath condensation, snow or mud on the glass.
  - (d) M.
- **Muzzle-blast obscuration**
  - (a) `optic_blast` is written (game.rs:26173-26195) but ignored by the renderer (renderer.rs:2134-2136), which also silently disables the smashed-sight smear.
  - (d) M.
- **Snow glare**: none. (d) S–M.
- **Compass**: the needle simply shows the camera's heading (hand_compass.rs:49-51). There is no deviation from 57 t of steel and no declination. (d) S.

---

## 13. Radio and communication
- **(a) What the code does**
  - `ModuleKind::Radio` exists (game.rs:5137, 5192). It takes shock damage (tiger_damage.rs:44, 350) and is repaired in 2 s (game.rs:7204). Only a debug readout reads it (mechanic_readout.rs:23).
  - There is no FuG 5 or FuG 2 set, intercom, throat microphone, radio net, range, jamming or orders.
  - LAN carries only action flags (lan.rs:48-57).
  - The Enigma is a static prop (game.rs:492-503, 16374-16377).
  - The only "radio" in the game is **written text**, which conflicts with the first principle:
    - the HUD line "RADIO SILENCE" (ui.rs:1910);
    - the field orders (winter_mission.rs:72-78);
    - the defeat line "No one answered the intercom." (ui.rs:2076).
- **(b) Wordless alternatives**
  - The FuG 5 heard through the headset: carrier hiss, squelch, Morse, and unintelligible voices that fade with range, terrain, antenna damage and battery, until radio silence is literal silence.
  - Flare-pistol colours and signal flags as the visible channel.
  - *Inferred/general, verify:* an Enigma was headquarters or command equipment, not standard in a line Tiger.
- **(c) Why it matters:** a lone Tiger cut off from its net *is* the story, and German armour's radio discipline was a decisive edge.
- **(d) Effort:** M for radio audio and the consequences of damage; S to replace the text.

---

## 14. Bugs, contradictions and ledger gaps (quick fixes, mostly S)

| # | Finding | Where |
|---|---|---|
| 1 | Enemy dispersion is added along world X/Y, not across the line of fire. A T-34 firing along ±X has no left-right spread. | game.rs:23795-23800 |
| 2 | Roof and belly contacts are discarded. A shallow top entry is re-scored as a side or front plate hit from inside the hull. | game.rs:28684-28686, 28632-28643 |
| 3 | Sponson ammo racks are parented to the turret (probable; inferred from geometry). | interior_rig.rs `SHELL_SLOTS`; game.rs:16425, 20376-20380 |
| 4 | Every shell is deleted at 1,500 m. | game.rs:2605 |
| 5 | Fuel burn is 11.5–13.6 L/km, about 1.5–3× the repo's own spec. | standalone_fuel.rs:15; TIGER_I_SPEC.md:18 |
| 6 | Night is 20 % of the Sun, lit from about 35° elevation: a brightness floor. | renderer.rs:6591, 809-816 |
| 7 | Speed of sound is 343 m/s (a 20 °C value) in a −25 °C world. | pressure_front.rs:8 |
| 8 | The 88 is heard only to 900 m, the 45 mm to 1,500 m; campaign tanks hear nothing. | game.rs:21637; at_guns.rs:438; live_playtest.rs:97 |
| 9 | BR-240P is typed as AP, and tungsten-core rounds use the full calibre for the thickness-to-calibre ratio. | ballistics.rs:715-725, 346 |
| 10 | 4 APCR rounds are stocked, but PzGr 40 cannot be loaded. | campaign.rs:157; reload.rs:9 |
| 11 | A door leaf stops an 88 mm round. | game.rs:21694-21697 |
| 12 | Wrecks are transparent to shells but block sight with an oversized box. | game.rs:21708-21737, 24805-24814 |
| 13 | The near-miss snow puff appears under the closest approach, not where the shell landed. | game.rs:22296-22303 |
| 14 | On-foot gravity is 14 m/s². | foot.rs:28 |
| 15 | Fire grows slower with the hatch open (*inferred* to be backwards). | surface_fire.rs:1253 |
| 16 | Armour-scar eviction heals old holes after 24 newer scars. | armor_damage.rs:12, 171-173 |
| 17 | Medkit instantly heals +34 HP. | game.rs:18204-18208 |
| 18 | MG ammunition and extinguishers are never consumed. | campaign.rs:158, 163 |
| 19 | Dead Soviets drop 9 mm rounds. | game.rs:18227-18254 |
| 20 | `Strike.energy_kj` and `.penetrated` are never read. | tiger_damage.rs:207-208 |
| 21 | Transmission repaired with a hammer in 7 s; a track in 8 s. | game.rs:7198-7207 |
| 22 | Anti-tank guns have no night factor and read a global alert state. | at_guns.rs:364-367 |
| 23 | Spawned tanks face the player's hidden position. | game.rs:12899 |
| 24 | The AI traverse comment says 23°/s; the value is 45.8°/s. | tank.rs:19-22 |
| 25 | The driver's relay camera sits outside the armour and is not gated. | tiger_driver_visor.rs:13-27 |
| 26 | Written orders and HUD text break the wordless principle. | winter_mission.rs:72-78; ui.rs:1910, 2076 |
| 27 | Anachronisms: Panzerfaust 60 (Act 1), SU-100 swarm, KV-2 boss, IS-3, BR-350B (verify). | §9.4 |
| 28 | Non-physical audio: shell shriek, fixed near-miss clip, ambient booms. | audio.rs:183-191; game.rs:22297-22303, 15312-15328 |
| 29 | Stale comments: "Face-hardened" on an RHA table. | ballistics.rs:219 |

**Ledger hygiene (S).**
- Add rows to PHYSICAL_TRUTH_AUDIT.md for:
  - vehicle movement;
  - the one-crewman station abstraction;
  - the 0.24 J lethality floor and the 220 m/s fragment cap;
  - 2.8 J ammunition initiation and whole-load detonation;
  - the 0.0025 blast coupling;
  - collision-free fragments and fire anchoring;
  - sound levels and propagation;
  - the fuel law;
  - the wreck sight boxes;
  - weightless supplies and the medkit;
  - scar eviction.
- Retire the stale armour P0 (PHYSICAL_TRUTH_AUDIT.md:71, against game.rs:24184-24187).
- Update docs/ACOUSTICS.md:7 ("spectacular, not accurate") to the owner's loudness rule.

---

## 15. Top 15: what to simulate next

Ranked by player-felt impact × historical importance, with dependencies noted.

1. **Physical sound.** §1.8: calibrated source levels, propagation in dB, the Tiger as an acoustic enclosure with the Maybach and tracks at true level, and an ear driven by pressure at the ear.
   - Why: the owner's explicit rule, and it is felt every second. For a real crew, the world was engine roar plus intercom, and head-out listening was the commander's main sensor.
   - Effort: **L** overall. A first cut is **M**: a calibration table, the dB distance law, hull transmission loss, and removing the comfort caps.
   - Unlocks: item 11.
2. **A power-limited drivetrain.** Olvar gears, one real RPM value, L 600 C steering, and resistance and grip per surface.
   - Why: it is felt in every second of driving, and it is the Tiger's defining trait (ponderous and thirsty). It feeds the engine sound, fuel burn and turret traverse. **M.**
3. **Survivable wounds for the lone crewman.**
   - Behind-armour debris at physical speeds with real lethality.
   - Anatomical wounds, bleeding, shock, treatment.
   - A hittable head-out commander.
   - One injury model on foot and in the tank (no instant medkit).
   - Why: today every hole is game over, via 0.24 J. The design is already locked in STATUS. **L.**
4. **A shared damage state for enemy vehicles.** Modules, crews, bail-outs, immobilisation, gun and optic kills, and cook-offs derived from what was actually hit.
   - Why: every engagement; the ledger's own P0; and bailing crews are wordless evidence. **M–L.**
5. **Complete armour geometry.** Roof and belly plates; a roofline cut so the mantlet is actually struck; the 60 mm lower side with road wheels as spaced armour; a true 3D impact angle including hull tilt.
   - Why: every hit. It is a prerequisite for mines, grenades and plunging fire. **S–M.**
6. **88 mm gunnery.** The binocular TZF 9b (2.5×) with range scales per ammunition; flight beyond 1.5 km; gun dispersion and jump; air density and propellant temperature; a loadable PzGr 40.
   - Why: the 88's reach was the Tiger's identity. **S–M.**
7. **The 1942–43 infantry anti-tank threat.** PTRD/PTRS rifles that blind vision blocks, wreck MGs and hit an exposed commander; mines; bottle and grenade teams on the engine deck.
   - Why: Zabel's report shows this is how Tigers were actually hurt in early 1943. The damage table's rifle class is already waiting. **M–L.**
8. **Temperature as simulated state.** Cold starts (inertia starter, preheating, coolant), crew cold and frostbite, frosted optics, the speed and absorption of sound, drag and muzzle velocity, ice strength.
   - Why: winter is the setting, and one scalar unlocks many couplings. **S** for the scalar, **M** for the consequences.
9. **Hull attitude and suspension.** Sample both track lines; springs and dampers; slope, step and trench limits; rollover; gun cant and hull-down depression; spread on the move taken from hull motion.
   - Why: balka fighting and hull-down duels. **M.**
10. **One man, five stations, embodied.** Timed movement between posts through the interior, and no driving from the sight: the tank coasts or brakes when the driver's seat is empty. At minimum, name the current abstraction in the ledger.
    - Why: this is the central tension of a lone crewman. **M** (S just to name it).
11. **AI perception from the same physics.**
    - Hearing from the acoustics model: engine, tracks and gun, masked by the storm and shaped by wind.
    - Sight from the rendered extinction, night, whitewash, exhaust, snow spray and tracks.
    - Range-estimation error; the dispersion fix; campaign tanks that can hear.
    - Why: every encounter, and all of stealth play. **M**, after item 1.
12. **Indirect fire.** Mortars and artillery with observers and adjustment, replacing the scripted ambient booms.
    - Why: the dominant threat and soundscape of 1942–43, and it forces crews to button up. **M–L.**
13. **Snow depth, ice and water.** A snow-depth field that affects vehicles and men on foot (bellying, the T-34's advantage, walking at knee depth); river ice with load limits; fording; bridge ratings.
    - Why: the Don, Aksai, Myshkova and Manych crossings. **M–L.**
14. **Real night and sky.** An ephemeris for the act dates (8 h of daylight, a noon sun near 18°); moon and starlight at true levels; flares and fires as the night's light; dark adaptation and flash blindness; removing the 20 % "night" floor.
    - Why: 16 h of darkness per day, and the floor is a CLAUDE.md violation. **S–M.**
15. **Logistics and reliability.** Calibrated fuel burn (now); MG ammunition and extinguisher use; the weight and bulk of supplies; breakdowns (final drives, transmission); recovery, towing and demolition; spares consumed by repairs.
    - Why: in 1943, fuel, spares and recovery governed how many Tigers could fight. **M–L** (S for the fuel recalibration).

**Next tier (16–21)**

16. Fire and ammunition physics: an engine bay and firewall; the automatic extinguisher; propellant burn versus detonation; fire and smoke harming the crew; correcting the effect of venting.
17. A period-correct Soviet order of battle (KV-1/KV-1S, T-70, T-60, Lend-Lease tanks, ZiS-3, 85 mm), with the §9.4 anachronisms removed.
18. Suppression, morale, surrender and bail-out, for infantry and crews.
19. Radio and intercom as wordless sound, replacing written orders and HUD text.
20. Period-correct optics and weather on glass: drum cupola, binocular TZF 9b, KFF 2, KZF 2; frost, fogging and snow; gating the driver's outside camera.
21. Destruction:
    - runtime craters and rubble with collision;
    - destructible map buildings;
    - source-derived collision for the ~3,100 placements that have none;
    - crushing anti-tank guns and soldiers.

---

## 16. What this audit did not do
- Nothing was run: no build, test, capture or listening pass. Every level in §1.7 comes from the owner's own fixture measurements (docs/IMPACT_SOUND_20260928.md) or from arithmetic on constants.
- Historical and acoustic figures are general-knowledge orientation values, not sourced in this pass.
- Counts over manifest placements (§11) come from one read of the Act I manifest by a sub-audit and are marked *inferred*.

## 17. Citations re-verified directly by me
- **Sound (audited directly):** acoustics.rs:7-11, 585, 625, 844-860, 1422, 1740-1827, 1899-1976, 2010-2036, 2054-2073, 2193-2251; audio.rs:173-210, 1369-1379, 1821-1936, 2107-2188; audio_ear.rs:1-120; audio_feedback.rs:195-203; weather_audio.rs:1-222; spatial_audio.rs:207-211, 276-333; game.rs:473, 592-633, 15080-15130, 15200-15330, 20015-20032, 21524-21536, 22195-22310, 22470-22500, 22740-22820, 23790-23830, 24050-24095; winter_encounter_ai.rs:180-198.
- **Spot-checks of the sub-audits:**
  - tank.rs:14-32, 733-741, 846-905, 80-90; standalone_fuel.rs:1-30.
  - live_playtest.rs:92-102; game.rs:21630-21640; campaign.rs:155-165, 848-856; infantry_combat.rs:474-488.
  - game.rs:28676-28690; ballistics.rs:216-228, 462-512, 712-728; game.rs:2600-2608, 3262-3270, 5997, 22330-22346.
  - game.rs:13810-13832, 21690-21722, 22856-22880; tiger_driver_visor.rs:10-30; ui.rs:1906-1912, 2074-2078; winter_mission.rs:68-82.
  - winter_weather.rs:1-40; renderer.rs:805-818, 6586-6594; foot.rs:24-30; world.rs:56-74.
  - tiger_damage.rs:207-208, 424; surface_fire.rs:1248-1256; game.rs:24815-24830, 16418-16432, 20372-20384, 18200-18210, 18226-18256, 12894-12901, 24183-24190; interior_rig.rs:100-130 (with a script over all 65 `SHELL_SLOTS`); stealth.rs:16-23; ammunition_fire.rs:1-8; campaign mg_rounds/extinguishers grep; TIGER_I_SPEC.md:14-34.
