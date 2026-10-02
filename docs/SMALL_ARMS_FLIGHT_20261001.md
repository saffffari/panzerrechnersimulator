# Small-arms flight, selected tracers and source-contact sparks

The live path still uses the authoritative 240 Hz swept projectile clock. It queries geometry
at flight time; it never stores a promised hit at launch. Tank armor remains inert to these ball
profiles. Player armor uses retained articulated Tiger source triangles and exposed crew uses
its physical pose. The pre-existing source-bounds approximation for other vehicles remains a
separate limitation. This change does not convert it into exact mesh collision.

## Profiles and source boundary

| Live family | Caliber | Bullet mass | Launch speed | Exponential drag k |
|---|---:|---:|---:|---:|
| P08 ball | 9 mm | 8.0 g | 350 m/s | 0.0018 /m |
| German rifle / MG34 | 7.92 mm | 12.8 g | 760 m/s | 0.0012 /m |
| M1891/30 light ball | 7.62 mm | 9.6 g | 865.0224 m/s | 0.0015 /m |
| DT family | 7.62 mm | 9.6 g | 844.6008 m/s | 0.0015 /m |

The Soviet launch references are the US GHQ Far East Command intelligence handbook,
[*Uniform, Insignia, Equipment, North Korean Army*, August 1950](https://www.cuttersguide.com/pdf/Military-and-Uniforms/Uniform,%20insignia,%20equipment,%20North%20Korean%20Army,%20August%201950.pdf),
printed pages 19 and 26: M1891/30 at 2838 ft/s and DP ball at 2771 ft/s. This is a postwar weapon
identification source, not a measurement of the game's 1942 cartridge lot. DT deliberately uses
the DP-family launch reference as a proxy. The German 760 and P08 350 launch choices are retained
from the existing game; the [US MIS 1943 MG34 manual](https://www.lonesentry.com/manuals/german-infantry-weapons/mg34-machine-gun.html)
only establishes a broad ammunition-dependent 2500–3000 ft/s range, not an exact fit for 760.
All masses and drag coefficients here remain explicit engineering estimates, not calibrated
cartridge tables. Gravity, wind and elapsed flight use the existing shared solver. This is not
a G1/G7 or six-degree-of-freedom model.

## Presentation

Rifle and pistol ball ammunition is unlit. The authored MG belt mix selects every fifth round
per shooter/team/weapon counter; unrelated shooters do not change that sequence. Coax/hull MG
share the current player MG ammunition identity. This is a game loadout choice, not a claim
about a universal historical belt recipe. Selected rounds use the same family flight profile
until a tracer-specific drag/charge reference is available. The amber presentation is shared;
it does not claim measured cartridge spectra. Visible composition is limited to 800 m without
retiring or modifying the projectile.

Tracer segments are the actual 240 Hz swept segments, clipped at real contacts and to an 8.333 ms
exposure window at absolute renderer time. No muzzle-to-hit interpolation, manufactured endpoint,
or minimum travel duration is used by live bullets. The ordinary continuous tracer geometry
remains separate from one-native-pixel fire/spark grain presentation. Its width is a bounded
raster visibility approximation, not bullet collision diameter. A 4096-segment capacity and
omission counter make overload explicit.

Armor sparks originate 8 mm outside the struck source plane. Their count and speed respond to
actual kinetic energy and incidence; an outward hemisphere/reflection and full cuboid support
check keep the cosmetic grains outside that plane. Tiny 1.5–3.5mm grains and at most 4 cm exposure
tails feed OLD's native-pixel HDR renderer. MAIN retains its existing effect geometry renderer.
These are cosmetic incandescent fragments, not a calibrated heat-transfer simulation or extra
lethal spall. They create neither an explosion flash nor smoke. Main-gun fire/smoke is unchanged.

The new contact grains use the actual substep contact time, so 30/60/144 Hz render partitions have
the same age at a common timestamp. OLD's weapon VFX already consumed full dt; MAIN's old 0.1 s
presentation clamp was removed. Generic pre-existing muzzle/blast creation is still frame-bound;
this does not claim a fully subframe-timestamped pyrotechnics system.

`capture_small_arms_receipt` records bounded 512 recent events keyed by projectile ID and monotonic
launch, selected tracer, source-armor contact, spark burst and submitted-trail counters. Submission
is not visible-pixel evidence. Native visual/audio capture remains required after CPU checks.
