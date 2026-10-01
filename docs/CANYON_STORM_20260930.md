# Storm combat, cubic fire and interior replay

Native captures are in `captures/`; the Pi comparison page is `canyon_storm_review_20260930.html`
in `Downloads/Panzerrechnersystem_Canyon_v2_2026-09-30/`.

## Weather and combat

`tools/worldgen/canyon_weather_study.py` makes separate workshops from canonical v2. Terrain,
materials, route, buildings and encounter data remain the same. Default v2 weather is unchanged.

| Study | UTC on 20 January 1943 | Fog control | Cloud cover | Storm control |
| --- | --- | --- | --- | --- |
| Existing v2 | 06:35 | 0.0022 | 0.82 | 0.07 |
| Storm morning | 06:35 | 0.0046 | 0.98 | 0.58 |
| Late afternoon still | 12:45 | 0.0046 | 0.98 | 0.58 |
| Low-sun snowfall | 13:20 | 0.0046 | 0.98 | 0.58 |
| Rejected dark test | 14:15 | 0.0046 | 0.98 | 0.58 |

The two selected 18-second combat clips use the same seed-29 Gaea course. This is a disclosed
lighting fixture: the Tiger starts at a viable point near 180 m from the first Soviet 45 mm gun.
The gun's crew knows the engagement has begun. The commander opens the hatch, rides head-out,
and the Tiger fires two staged loaded shots. Soviet aiming, movement, shell flight, armor damage
and the destruction of the gun use the ordinary simulation. The Tiger takes a rear ricochet and
track damage. These are combat render studies, not proof of an autonomous full Canyon clear,
manual-reload gameplay or a 1–2 hour balanced level.

`canyon_storm_day_combat_20260930.mp4` and `canyon_storm_snowfall_combat_20260930.mp4` pass
all required action and outcome checks. Both contain 18 seconds of H.264 video and native AAC
audio, at 1920×1080/24 fps from a 640×360 sensor. Their muzzle stills are extracted at 5.5833 s;
quiet exterior stills at 8 s. The separate player-view PNGs render at 1280×720, enlarged 3×.

The low-sun comparison reduces the background relative to combat emission while preserving
readable ground. The later blue-hour test is almost black between flashes and was not selected
as a gameplay target. Eye adaptation stays active; no exposure override or postproduction color
grade is used. Existing broad terrain sky-occlusion and material-aware ground-bounce limitations
remain. More fog alone does not resolve those lighting limitations.

Reproduction, after creating the studies and building release:

```sh
KESSEL_RENDERER=shader KESSEL_INTERNAL_RES=640x360 KESSEL_GRAPHICS_PRESET=high \
KESSEL_CANYON_SEED=29 KESSEL_CAPTURE_IDLE_FACTOR=0 KESSEL_CAPTURE_FRAME_PAUSE_MS=1 \
target/release/panzerrechnersystem mechanic-review captures/new_storm_take \
catalog/shader_reviews/canyon_storm_snowfall_combat_20260930.v1.json
```

## Fire and heat study

Pre-Canyon generation-held cubes are restored. Cabin edges are 6–8 mm instead of 8–10 mm;
exterior edges are 80–115 mm instead of 160–230 mm, with 256 instead of 64 grains per source.
The faster surface spread and larger foam remain. The older flame radiance table is restored;
interior aggregate illumination still derives from actual grain face area and color.

The four-second `canyon_finefire_base_20260930.mp4` and `canyon_finefire_heat_20260930.mp4`
show the same seeded surface ignition. Both pass their checks and finish with identical complete
simulation telemetry, including nine burning patches. This is a staged ignition, not a naturally
occurring combat penetration. Black cubic soot remains conspicuous and needs further review.

Heat shimmer is a bounded screen-space approximation, compiled behind the debug-only
`KESSEL_DEBUG_HEAT_HAZE` gate. It is disabled in both shipping presets; the regression also
checks that setting the variable in a release build cannot enable it. For the review binary:

```sh
cargo rustc --offline --locked --release --bin panzerrechnersystem -- -C debug-assertions=yes
```

Capture the fire scenario with `KESSEL_DEBUG_HEAT_HAZE=0` and `=1` in separate processes.
Rebuild ordinary release afterward. The pass bends existing background imagery above active
fire sources; it adds no light and does not alter spreading, suppression or damage. It is not a
thermal-fluid or full refractive transport simulation. Off-screen information is unavailable.

## Terminal replay

`canyon_interior_terminal_final_20260930.mp4` is an 11.5-second native terminal fixture: 1.5-second
fade, eight-second interior aftermath, then two seconds of black. The initial fade and every replay
phase use the previous rear-turret witness camera at 80° vertical FOV. It follows the actual turret
transform and never depends on the player's last station. All crew rigs are eligible for display;
no one is hidden merely because they were the embodied player.

A disclosed fatal ammunition hit creates the anatomical records. Only body, blood and fragment
presentation advance during replay. The fixture verifies the same five recorded crew outcomes
before and after; damage is not reapplied. The explosion still briefly clips the cabin, but the
3.2-second aftermath span lets the flash clear well before the eight-second replay finishes.

## Validation

Release build passed. The full host suite passed 1,427 tests with 54 ignored after ten local-socket
tests blocked by the sandbox were rerun successfully on the host. Focused replay and heat shader
checks were rerun after the final timing change. Native video streams were decoded for sampled
frame review, action/outcome receipts were checked, and delivered media has remote SHA-256
verification. This establishes implementation and recorded behavior, not final visual approval.
