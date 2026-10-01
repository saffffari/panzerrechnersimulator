# Blizzard firefight and dust correction — 2026-10-01

The large cube in the effect review was the legacy `SnowPuff` box spawned by
`spawn_blast -> spawn_dust`, not the new smoke density. Dust and kicked-up snow now
use soft finite supports in the existing extinction/scattering renderer. Their
old box slots stay hidden. Shared weather wind advects the dust; real solid
debris is unchanged. Expansion is a prescribed presentation envelope and the
shared twelve-volume budget can omit low-priority dust. See the physical-truth
audit for the rendering limits.

Matched 2.2-second before/after frames are
`captures/impact_cube_before_20261001.png` and
`captures/impact_cube_after_20261001.png`. The six-second regression film is
`captures/impact_motion_dust_fix_20261001.mp4`.

## Actual combat capture

`blizzard-battle` records an external witness of a continuous winter Arena fight.
Two baseline-policy T-34s start on terrain/tree/cover-checked lanes. A paced
automated Tiger crew lays the gun and services the physical breech. Both sides
use normal trigger acceptance, swept projectiles, armor, damage, collision and
wreck presentation. Opening positions and alerted enemies are explicit setup;
no impacts, kills, or invulnerability are injected. This is automated combat,
not a human playthrough or a real-time performance benchmark.

The coupled weather override is 2.5: snow, cloud, wind and fog evolve together.
The camera follows the first T-34 for seven seconds, then observes the Tiger
before the next incoming shot. Rendering restores the gameplay view afterward.
The native sensor is 640×360, presented at 1280×720, 30 fps. Sound is a
post-produced exterior mix synchronized to accepted gun sequences and actual
destruction transitions; it is not a recording of the live player's audio mix.

```sh
KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_INTERNAL_RES=640x360 \
  target/release/panzerrechnersystem blizzard-battle \
  captures/blizzard_firefight_20261001 14
```

The command refuses existing outputs. Add `preview` after the duration for a
five-frame-per-second camera check. The JSON beside the video records setup,
accepted shots, hit counters, destruction events, weather and final combat state.
Capture acceptance requires both sides to fire and an authoritative resolved hit.
The final delivery receipt additionally records media decoding and actual kills.

## Track and audio revision

The second recording is `captures/blizzard_tracks_audio_v2_20261001.mp4`.
Track bands are measured from the loaded source hull: Tiger 0.7706/0.7674 m wide,
T-34 0.7065 m, instead of the old 2.10 m circles. A persistent sparse atlas keeps
12.5 cm material samples at campaign scale. The atlas has 256 non-evicting pages;
once full, new pages receive no ruts. Compaction changes material/shading, not
terrain collision geometry. CPU source-width/page tests and production GPU
readback verify the footprint without widening it.

The shared cannon source replaces sustained hiss and a long sine body with
compact pressure and bass impulses, upper-bass support and a short crack. The
new observer mix includes four real armor contacts previously absent from the
film, four discharges and two destructions. Its wind is the actual dynamic
weather voice; engine level follows the observed speed and range. Propagation,
the reproduction curve and a single master limiter replace arbitrary per-film
gains and stacked saturation. This remains an offline external microphone mix;
the receipt lists its approximations. Source and mix tests assess spectrum,
onset, level and event inclusion; they do not claim subjective listening approval.
