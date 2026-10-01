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
