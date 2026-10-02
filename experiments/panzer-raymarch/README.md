# Panzer raymarching experiment

An isolated, runnable rendering study inspired by the user's deltaP project. The host and palette
pass were adapted from that local project; `shaders/march.wgsl` is a new analytic Tiger-like scene.
It does not replace Panzer's renderer, source armor meshes, or gameplay.

## Run

From `/home/alex/panzer/canyon-v2-20260930`:

```sh
CARGO_TARGET_DIR=target/raymarch cargo run --offline --manifest-path experiments/panzer-raymarch/Cargo.toml
```

WASD moves, Q/E descend/ascend, Shift moves faster, hold right mouse to look, R resets camera,
N switches day/night, P toggles palette, 1–4 select no/perturb/greedy-pair/best-pair dither,
comma/period change dither strength, brackets adjust exposure, minus/equals adjust palette-pair
penalty, F11 toggles fullscreen. Shader edits reload live; errors retain the last working pipeline.

```sh
target/raymarch/debug/panzer-raymarch --shot /home/alex/panzer/canyon-v2-20260930/captures/panzer_raymarch_custom.png --width 320 --height 180 --scale 3 --palette 1 --dither 3 --time 0
```

Use a new capture filename each time. `--time 100` selects night in headless mode. All comparisons
hold geometry, camera, exposure and native resolution fixed unless the filename says otherwise.

## What it tests

One shared analytic scene supplies primary visibility, sun/emitter shadows, one diffuse bounce and
one reflected continuation. An exact ground-plane intersection avoids grazing-ray marching stalls.
The tank includes authored hull, track openings, road wheels, turret, mantlet, barrel/muzzle brake,
cupola, hatches, exhausts and grilles. A gate, finite warm emitter and reflective sphere expose
shadow and reflected-visibility errors. HDR lighting is separate from ACES and PICO-8 ordered dither.

This is a small numerical experiment, not a complete physically based renderer: authored materials
and emitters, 384-step tracing at explicit tolerance, four deterministic diffuse/area samples, one
bounce, and ideal reflected continuation approximate the integral. Unresolved primary/secondary rays
are magenta; shadow nonconvergence is conservative and can darken a path. No production assets,
weather simulation, damage, sound or combat are present. It tests coherent visibility and presentation;
the native combat suite tests the game itself.

A silent six-second orbit can be recorded with the same renderer (no video interpolation):

```sh
target/raymarch/debug/panzer-raymarch --video /home/alex/panzer/canyon-v2-20260930/captures/panzer_raymarch_orbit_custom.mp4 --seconds 6 --fps 24 --palette 1 --dither 3
```
